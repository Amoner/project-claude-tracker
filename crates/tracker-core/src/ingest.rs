//! Handle a single Claude Code hook event. Called from `tracker-cli ingest <event>`.
//!
//! On ANY unexpected error we log to `~/.claude-tracker/logs/ingest.log` and
//! exit 0 from the caller so Claude Code is never affected.

use std::io::Read;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Deserialize;

use crate::db::{self, Db, Touch};
use crate::paths;

#[derive(Debug, Deserialize)]
struct HookPayload {
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(flatten)]
    _rest: std::collections::HashMap<String, serde_json::Value>,
}

/// Parse the JSON payload and write ingestion results to the DB. Returns the
/// upserted project path, if we could derive one.
pub fn ingest_event(event_name: &str, stdin_raw: &str, db: &Db) -> Result<Option<PathBuf>> {
    let payload: HookPayload = if stdin_raw.trim().is_empty() {
        HookPayload {
            session_id: None,
            cwd: None,
            _rest: Default::default(),
        }
    } else {
        serde_json::from_str(stdin_raw)
            .with_context(|| format!("parsing hook JSON for {event_name}"))?
    };

    let Some(cwd) = payload.cwd.as_deref().filter(|s| !s.trim().is_empty()) else {
        // No cwd → nothing actionable. Still record as a no-op event in the
        // unlikely future case that we want to count them.
        return Ok(None);
    };

    let path = PathBuf::from(cwd);

    // Subagent worktrees under `<repo>/.claude/worktrees/` are ephemeral and
    // shouldn't pollute the project list. The hook still returns success so
    // Claude Code isn't affected.
    if paths::is_ephemeral_worktree(&path) {
        return Ok(None);
    }

    let name_hint = paths::project_name_from_path(&path);

    let project_id = db.upsert_project_by_path(&path, &name_hint)?;
    let now = Utc::now();
    let session_id = payload.session_id.as_deref();

    db.record_event(project_id, session_id, event_name, stdin_raw, now)?;

    let kind = match event_name {
        "SessionStart" => Touch::SessionStart,
        "UserPromptSubmit" => Touch::Prompt,
        _ => Touch::LastActive,
    };
    db.touch(project_id, now, kind)?;
    Ok(Some(path))
}

/// Convenience wrapper: read all of stdin, parse, write, swallow errors into
/// a log file. Returns `Ok(())` regardless of success so callers can `exit(0)`
/// from inside a Claude Code hook.
pub fn ingest_from_stdin(event_name: &str) -> Result<()> {
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf).ok();
    let db = db::open_db()?;
    if let Err(e) = ingest_event(event_name, &buf, &db) {
        log_error(event_name, &e, &buf);
    }
    Ok(())
}

fn log_error(event_name: &str, err: &anyhow::Error, payload: &str) {
    paths::append_log(
        "ingest.log",
        &format!("event={event_name} err={err:#}\npayload={payload}\n---"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_at;

    #[test]
    fn session_start_upserts_and_increments() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_at(&dir.path().join("t.sqlite")).unwrap();
        let payload = serde_json::json!({
            "session_id": "s1",
            "cwd": "/tmp/my-project",
            "hook_event_name": "SessionStart"
        })
        .to_string();
        let path = ingest_event("SessionStart", &payload, &db).unwrap().unwrap();
        assert_eq!(path, PathBuf::from("/tmp/my-project"));
        let p = db.get_project_by_path(&path).unwrap().unwrap();
        assert_eq!(p.sessions_started, 1);
        assert!(p.last_active_at.is_some());
    }

    #[test]
    fn prompt_submit_increments() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_at(&dir.path().join("t.sqlite")).unwrap();
        let payload = serde_json::json!({
            "session_id": "s1",
            "cwd": "/tmp/proj",
            "hook_event_name": "UserPromptSubmit",
            "prompt": "hi"
        })
        .to_string();
        ingest_event("UserPromptSubmit", &payload, &db).unwrap();
        ingest_event("UserPromptSubmit", &payload, &db).unwrap();
        let p = db.get_project_by_path(&PathBuf::from("/tmp/proj")).unwrap().unwrap();
        assert_eq!(p.prompts_count, 2);
    }

    #[test]
    fn ephemeral_worktree_cwd_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_at(&dir.path().join("t.sqlite")).unwrap();
        let payload = serde_json::json!({
            "session_id": "s1",
            "cwd": "/Users/x/Documents/AoG/.claude/worktrees/agent-abc",
            "hook_event_name": "SessionStart"
        })
        .to_string();
        let out = ingest_event("SessionStart", &payload, &db).unwrap();
        assert!(out.is_none());
        assert!(db.list_projects(true).unwrap().is_empty());
    }

    #[test]
    fn missing_cwd_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_at(&dir.path().join("t.sqlite")).unwrap();
        // Valid JSON but no `cwd` field.
        let payload = r#"{"session_id":"s1","hook_event_name":"Stop"}"#;
        let out = ingest_event("Stop", payload, &db).unwrap();
        assert!(out.is_none());
        assert!(db.list_projects(true).unwrap().is_empty());
    }

    #[test]
    fn whitespace_cwd_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_at(&dir.path().join("t.sqlite")).unwrap();
        let payload = r#"{"cwd":"   ","hook_event_name":"SessionStart"}"#;
        let out = ingest_event("SessionStart", payload, &db).unwrap();
        assert!(out.is_none());
        assert!(db.list_projects(true).unwrap().is_empty());
    }

    #[test]
    fn malformed_json_errors_but_does_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_at(&dir.path().join("t.sqlite")).unwrap();
        // ingest_event surfaces the parse error; the public wrapper
        // `ingest_from_stdin` swallows it into the log file.
        let out = ingest_event("SessionStart", "{not json", &db);
        assert!(out.is_err());
        assert!(db.list_projects(true).unwrap().is_empty());
    }

    #[test]
    fn empty_payload_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_at(&dir.path().join("t.sqlite")).unwrap();
        let out = ingest_event("SessionStart", "", &db).unwrap();
        assert!(out.is_none());
        assert!(db.list_projects(true).unwrap().is_empty());
    }
}
