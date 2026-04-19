//! Safe merging of tracker hooks into `~/.claude/settings.json`.
//!
//! `serde_json::Value` uses preserve_order via the crate feature so we
//! round-trip user settings without reordering keys.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde_json::{json, Value};

use crate::paths;

/// Substrings both required in the `command` field for us to consider a hook
/// entry ours. Using two tokens (not one combined string) lets us match
/// commands where the binary path is shell-quoted — e.g. a path that contains
/// a space gets wrapped in `"..."`, which separates the binary name from
/// `ingest`.
const MARKER_BIN: &str = "tracker-cli";
const MARKER_SUB: &str = "ingest";

pub fn command_is_ours(cmd: &str) -> bool {
    cmd.contains(MARKER_BIN) && cmd.contains(MARKER_SUB)
}

/// The Claude Code events we install hooks for.
pub const INSTALLED_EVENTS: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "Stop",
    "CwdChanged",
];

#[derive(Debug, Clone)]
pub struct HookStatus {
    pub settings_path: PathBuf,
    pub installed_events: Vec<String>,
    pub cli_path: Option<PathBuf>,
}

pub fn status() -> Result<HookStatus> {
    let settings_path = paths::claude_settings()?;
    let mut installed = Vec::new();
    let mut cli_path: Option<PathBuf> = None;
    if settings_path.exists() {
        let raw = std::fs::read_to_string(&settings_path)?;
        let v: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
        if let Some(events) = v.get("hooks").and_then(|h| h.as_object()) {
            for event in INSTALLED_EVENTS {
                let entries = events
                    .get(*event)
                    .and_then(|e| e.as_array())
                    .cloned()
                    .unwrap_or_default();
                for entry in entries {
                    if let Some(inner) = entry.get("hooks").and_then(|h| h.as_array()) {
                        for h in inner {
                            let cmd = h
                                .get("command")
                                .and_then(|c| c.as_str())
                                .unwrap_or_default();
                            if command_is_ours(cmd) {
                                if !installed.contains(&event.to_string()) {
                                    installed.push((*event).to_string());
                                }
                                if cli_path.is_none() {
                                    if let Some(p) = extract_cli_path(cmd) {
                                        cli_path = Some(p);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(HookStatus {
        settings_path,
        installed_events: installed,
        cli_path,
    })
}

fn extract_cli_path(cmd: &str) -> Option<PathBuf> {
    // Two shapes we emit:
    //   /abs/no-space/tracker-cli ingest SessionStart
    //   "/abs/with space/tracker-cli" ingest SessionStart
    // The quoted shape uses backslash escapes for `"`, `\`, `$`, and backtick.
    let cmd = cmd.trim();
    if let Some(rest) = cmd.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => out.push(chars.next()?),
                '"' => return Some(PathBuf::from(out)),
                _ => out.push(c),
            }
        }
        return None;
    }
    let end = cmd.find(' ')?;
    Some(PathBuf::from(&cmd[..end]))
}

/// Install (or re-point) our hooks to use `cli_path`. Preserves every other
/// key in the settings file, writes atomically, and saves a one-shot backup
/// in `~/.claude/backups/` before touching anything.
pub fn install(cli_path: &Path) -> Result<HookStatus> {
    let settings_path = paths::claude_settings()?;
    let mut value = load_settings_json(&settings_path)?;
    backup_before_edit(&settings_path, &value)?;

    let hooks_obj = ensure_object(&mut value, "hooks");
    for event in INSTALLED_EVENTS {
        upsert_event_hook(hooks_obj, event, cli_path);
    }

    write_atomic(&settings_path, &value)?;
    status()
}

/// Uninstall: remove only the entries whose `command` looks like one of ours.
pub fn uninstall() -> Result<HookStatus> {
    let settings_path = paths::claude_settings()?;
    if !settings_path.exists() {
        return status();
    }
    let mut value = load_settings_json(&settings_path)?;
    backup_before_edit(&settings_path, &value)?;

    if let Some(hooks_obj) = value.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        let keys: Vec<String> = hooks_obj.keys().cloned().collect();
        for event in keys {
            let should_filter = INSTALLED_EVENTS.contains(&event.as_str());
            if !should_filter {
                continue;
            }
            let Some(entries) = hooks_obj.get_mut(&event).and_then(|e| e.as_array_mut()) else {
                continue;
            };
            entries.retain(|entry| !entry_is_ours(entry));
            // If the event has no entries left, drop the key entirely.
            if entries.is_empty() {
                hooks_obj.remove(&event);
            }
        }
    }

    write_atomic(&settings_path, &value)?;
    status()
}

fn entry_is_ours(entry: &Value) -> bool {
    let Some(inner) = entry.get("hooks").and_then(|h| h.as_array()) else {
        return false;
    };
    // Consider the entry ours if EVERY inner hook has our marker AND there's
    // at least one inner hook. Otherwise, leave it alone.
    if inner.is_empty() {
        return false;
    }
    inner.iter().all(|h| {
        h.get("command")
            .and_then(|c| c.as_str())
            .map(command_is_ours)
            .unwrap_or(false)
    })
}

fn upsert_event_hook(obj: &mut serde_json::Map<String, Value>, event: &str, cli_path: &Path) {
    let cmd = format!("{} ingest {}", shell_quote(cli_path), event);
    let new_entry = json!({
        "matcher": "",
        "hooks": [
            {
                "type": "command",
                "command": cmd,
                "timeout": 10
            }
        ]
    });

    let entries = obj
        .entry(event.to_string())
        .or_insert_with(|| Value::Array(vec![]))
        .as_array_mut();
    let Some(entries) = entries else {
        return;
    };
    // Remove ALL existing ours-entries, then push exactly one fresh one.
    entries.retain(|entry| !entry_is_ours(entry));
    entries.push(new_entry);
}

fn shell_quote(p: &Path) -> String {
    let s = p.to_string_lossy();
    if s.chars().all(|c| c.is_ascii_alphanumeric() || "/_.-".contains(c)) {
        return s.into_owned();
    }
    // Double-quoted shell literal. Inside double quotes POSIX shells still
    // interpret `$`, backtick, and `\`, so those need explicit escaping along
    // with `"`.
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        if matches!(c, '"' | '\\' | '$' | '`') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

fn load_settings_json(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let raw = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(json!({}));
    }
    let v: Value = serde_json::from_str(&raw)
        .map_err(|e| anyhow!("settings.json is not valid JSON: {e}"))?;
    Ok(v)
}

fn ensure_object<'a>(v: &'a mut Value, key: &str) -> &'a mut serde_json::Map<String, Value> {
    if !v.is_object() {
        *v = json!({});
    }
    let obj = v.as_object_mut().expect("checked object");
    obj.entry(key.to_string())
        .or_insert_with(|| Value::Object(Default::default()));
    obj.get_mut(key).unwrap().as_object_mut().unwrap()
}

fn backup_before_edit(path: &Path, _current: &Value) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let backups = paths::claude_backups_dir()?;
    std::fs::create_dir_all(&backups).ok();
    let stamp = Utc::now().format("%Y%m%dT%H%M%S").to_string();
    let dst = backups.join(format!("settings-{stamp}.json"));
    std::fs::copy(path, &dst).with_context(|| format!("backup to {}", dst.display()))?;
    Ok(Some(dst))
}

fn write_atomic(path: &Path, value: &Value) -> Result<()> {
    let serialized = serde_json::to_string_pretty(value)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &serialized)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_settings(v: Value) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
        (dir, path)
    }

    // Use private helpers directly rather than going through the paths
    // module, which is tied to $HOME.
    fn install_at(path: &Path, cli: &Path) -> Result<()> {
        let mut value = load_settings_json(path)?;
        let hooks = ensure_object(&mut value, "hooks");
        for event in INSTALLED_EVENTS {
            upsert_event_hook(hooks, event, cli);
        }
        write_atomic(path, &value)
    }

    fn uninstall_at(path: &Path) -> Result<()> {
        let mut value = load_settings_json(path)?;
        if let Some(hooks_obj) = value.get_mut("hooks").and_then(|h| h.as_object_mut()) {
            let keys: Vec<String> = hooks_obj.keys().cloned().collect();
            for event in keys {
                if !INSTALLED_EVENTS.contains(&event.as_str()) {
                    continue;
                }
                let Some(entries) = hooks_obj.get_mut(&event).and_then(|e| e.as_array_mut())
                else {
                    continue;
                };
                entries.retain(|entry| !entry_is_ours(entry));
                if entries.is_empty() {
                    hooks_obj.remove(&event);
                }
            }
        }
        write_atomic(path, &value)
    }

    #[test]
    fn preserves_existing_notification_hook() {
        let initial = json!({
            "env": { "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1" },
            "hooks": {
                "Notification": [
                    {
                        "matcher": "",
                        "hooks": [
                            { "type": "command", "command": "afplay /System/Library/Sounds/Glass.aiff" }
                        ]
                    }
                ]
            },
            "alwaysThinkingEnabled": true
        });
        let (_d, path) = temp_settings(initial.clone());
        install_at(&path, Path::new("/opt/tracker/tracker-cli")).unwrap();
        let after: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();

        // Non-hook keys untouched.
        assert_eq!(after["env"], initial["env"]);
        assert_eq!(after["alwaysThinkingEnabled"], json!(true));

        // Notification hook preserved verbatim.
        let notif = &after["hooks"]["Notification"][0]["hooks"][0]["command"];
        assert_eq!(notif, "afplay /System/Library/Sounds/Glass.aiff");

        // Our events installed.
        for event in INSTALLED_EVENTS {
            let entry = &after["hooks"][*event][0]["hooks"][0]["command"];
            assert!(entry.as_str().unwrap().contains("tracker-cli ingest"));
        }
    }

    #[test]
    fn install_is_idempotent_and_upgrades_cli_path() {
        let (_d, path) = temp_settings(json!({}));
        install_at(&path, Path::new("/v1/tracker-cli")).unwrap();
        install_at(&path, Path::new("/v2/tracker-cli")).unwrap();
        let after: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let cmd = after["hooks"]["SessionStart"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert!(cmd.starts_with("/v2/tracker-cli"));
        // Only one entry per event.
        assert_eq!(after["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn install_handles_paths_with_spaces() {
        let (_d, path) = temp_settings(json!({}));
        let cli = Path::new("/Applications/Claude Tracker.app/Contents/MacOS/tracker-cli");
        install_at(&path, cli).unwrap();
        let after: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let cmd = after["hooks"]["SessionStart"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert!(cmd.starts_with("\"/Applications/Claude Tracker.app"));
        assert!(command_is_ours(cmd));
        // Round-trip: the round-trip detection should see it as ours.
        let got = extract_cli_path(cmd).unwrap();
        assert_eq!(got, cli);
    }

    #[test]
    fn extract_cli_path_handles_both_shapes() {
        assert_eq!(
            extract_cli_path("/bin/tracker-cli ingest SessionStart"),
            Some(PathBuf::from("/bin/tracker-cli"))
        );
        assert_eq!(
            extract_cli_path("\"/Apps/Claude Tracker.app/Contents/MacOS/tracker-cli\" ingest Stop"),
            Some(PathBuf::from(
                "/Apps/Claude Tracker.app/Contents/MacOS/tracker-cli"
            ))
        );
    }

    #[test]
    fn uninstall_leaves_foreign_hooks_intact() {
        let initial = json!({
            "hooks": {
                "Notification": [
                    {
                        "matcher": "",
                        "hooks": [
                            { "type": "command", "command": "afplay /System/Library/Sounds/Glass.aiff" }
                        ]
                    }
                ]
            }
        });
        let (_d, path) = temp_settings(initial);
        install_at(&path, Path::new("/tracker-cli")).unwrap();
        uninstall_at(&path).unwrap();
        let after: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        // Our events are gone.
        for event in INSTALLED_EVENTS {
            assert!(after["hooks"].get(*event).is_none(), "leftover event {event}");
        }
        // Notification is still there.
        let notif = &after["hooks"]["Notification"][0]["hooks"][0]["command"];
        assert_eq!(notif, "afplay /System/Library/Sounds/Glass.aiff");
    }
}
