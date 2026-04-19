use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

/// Project display name from a path's last component. Returns `"(unknown)"`
/// when the path is empty, has no file name, or has a non-UTF-8 name.
pub fn project_name_from_path(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "(unknown)".to_string())
}

/// Expand a leading `~` to the user's home directory. Non-tilde input and
/// tilde-with-user forms (`~alice/…`) are returned unchanged.
pub fn expand_tilde(input: &str) -> PathBuf {
    if let Some(rest) = input.strip_prefix("~/") {
        if let Ok(h) = home() {
            return h.join(rest);
        }
    } else if input == "~" {
        if let Ok(h) = home() {
            return h;
        }
    }
    PathBuf::from(input)
}

pub fn home() -> Result<PathBuf> {
    dirs::home_dir().ok_or_else(|| anyhow!("could not locate home directory"))
}

pub fn claude_dir() -> Result<PathBuf> {
    Ok(home()?.join(".claude"))
}

pub fn claude_settings() -> Result<PathBuf> {
    Ok(claude_dir()?.join("settings.json"))
}

pub fn claude_projects_dir() -> Result<PathBuf> {
    Ok(claude_dir()?.join("projects"))
}

pub fn claude_backups_dir() -> Result<PathBuf> {
    Ok(claude_dir()?.join("backups"))
}

pub fn tracker_dir() -> Result<PathBuf> {
    Ok(home()?.join(".claude-tracker"))
}

pub fn tracker_db() -> Result<PathBuf> {
    Ok(tracker_dir()?.join("db.sqlite"))
}

pub fn tracker_logs_dir() -> Result<PathBuf> {
    Ok(tracker_dir()?.join("logs"))
}

pub fn ensure_tracker_dirs() -> Result<()> {
    std::fs::create_dir_all(tracker_dir()?)?;
    std::fs::create_dir_all(tracker_logs_dir()?)?;
    Ok(())
}

/// Best-effort append a timestamped line to a file inside the tracker logs dir.
/// Never returns errors — hooks and CLI failure paths must not compound.
pub fn append_log(file_name: &str, line: &str) {
    let Ok(dir) = tracker_logs_dir() else { return };
    std::fs::create_dir_all(&dir).ok();
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(dir.join(file_name))
    {
        let _ = writeln!(f, "[{}] {line}", chrono::Utc::now().to_rfc3339());
    }
}
