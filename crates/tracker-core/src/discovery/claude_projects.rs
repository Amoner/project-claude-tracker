//! Enumerate Claude Code's per-project directories to seed the tracker DB
//! with every project the user has ever opened in Claude Code.

use std::path::Path;

use anyhow::Result;
use chrono::Utc;

use super::{jsonl, DiscoveredProject};
use crate::paths;

pub fn scan_all() -> Result<Vec<DiscoveredProject>> {
    let root = paths::claude_projects_dir()?;
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&root)? {
        let Ok(entry) = entry else { continue };
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        match summarize_project(&dir) {
            Ok(Some(dp)) => out.push(dp),
            Ok(None) => {}
            Err(e) => tracing::warn!("skipping {}: {e}", dir.display()),
        }
    }
    Ok(out)
}

fn summarize_project(dir: &Path) -> Result<Option<DiscoveredProject>> {
    let jsonl_files: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .map_or(false, |x| x == "jsonl")
        })
        .collect();
    if jsonl_files.is_empty() {
        return Ok(None);
    }

    // cwd is stable across all transcripts of a project, and first/last_at use
    // min/max — all order-independent, so we iterate in read_dir order.
    let mut cwd = None;
    let mut first_at = None;
    let mut last_at = None;
    let mut sessions_started = 0i64;
    let mut user_prompts = 0i64;

    for f in &jsonl_files {
        // One .jsonl per Claude Code session.
        sessions_started += 1;
        if let Ok(summary) = jsonl::summarize_file(&f.path()) {
            if cwd.is_none() {
                cwd = summary.cwd;
            }
            if let Some(ts) = summary.first_at {
                if first_at.map_or(true, |f| ts < f) {
                    first_at = Some(ts);
                }
            }
            if let Some(ts) = summary.last_at {
                if last_at.map_or(true, |l| ts > l) {
                    last_at = Some(ts);
                }
            }
            user_prompts += summary.user_prompts;
        }
    }

    // If none of the transcripts yielded a cwd, fall back to best-effort decode.
    let path = match cwd {
        Some(p) => p,
        None => {
            let name = dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            crate::encode::best_effort_decode(name)
        }
    };

    let name = paths::project_name_from_path(&path);

    Ok(Some(DiscoveredProject {
        path,
        name,
        first_seen_at: first_at.unwrap_or_else(Utc::now),
        last_active_at: last_at,
        sessions_started,
        prompts_count: user_prompts,
    }))
}
