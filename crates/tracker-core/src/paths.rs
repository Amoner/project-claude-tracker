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

/// Claude Code's `using-git-worktrees` skill spins up throwaway worktrees at
/// `<repo>/.claude/worktrees/<slug>` for parallel subagent runs. They are not
/// projects in their own right and pollute the list, so we skip them during
/// automatic ingest/discovery. Manual `add_project_manual` is unaffected.
pub fn is_ephemeral_worktree(path: &Path) -> bool {
    let mut comps = path.components().peekable();
    while let Some(c) = comps.next() {
        if c.as_os_str() == ".claude" {
            if comps.peek().map(|n| n.as_os_str()) == Some(std::ffi::OsStr::new("worktrees")) {
                return true;
            }
        }
    }
    false
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ephemeral_worktree_matches_agent_dirs() {
        assert!(is_ephemeral_worktree(Path::new(
            "/Users/x/Documents/AoG/.claude/worktrees/agent-a0f53934fd7b0f9c1"
        )));
        assert!(is_ephemeral_worktree(Path::new(
            "/Users/x/Documents/AoG/.claude/worktrees/great-ptolemy"
        )));
        assert!(is_ephemeral_worktree(Path::new(
            "/Users/x/Documents/AoG/.claude/worktrees/agent-ac/src"
        )));
    }

    #[test]
    fn ephemeral_worktree_ignores_normal_paths() {
        assert!(!is_ephemeral_worktree(Path::new(
            "/Users/x/Documents/AoG"
        )));
        assert!(!is_ephemeral_worktree(Path::new(
            "/Users/x/Documents/AoG/.claude"
        )));
        assert!(!is_ephemeral_worktree(Path::new(
            "/Users/x/Documents/AoG/.claude/projects/foo"
        )));
        // A literal directory named `worktrees` outside `.claude/` is not ours.
        assert!(!is_ephemeral_worktree(Path::new(
            "/Users/x/worktrees/proj"
        )));
        // Lookalike folder names must not match.
        assert!(!is_ephemeral_worktree(Path::new(
            "/Users/x/.claudefoo/worktrees/y"
        )));
        assert!(!is_ephemeral_worktree(Path::new(
            "/Users/x/.claude/worktreesfoo"
        )));
    }

    #[test]
    fn ephemeral_worktree_trailing_slash_and_deep_nesting() {
        assert!(is_ephemeral_worktree(Path::new(
            "/Users/x/.claude/worktrees/"
        )));
        assert!(is_ephemeral_worktree(Path::new(
            "/Users/x/repo/.claude/worktrees/agent-ab/src/components/App.tsx"
        )));
    }

    #[cfg(windows)]
    #[test]
    fn ephemeral_worktree_windows_paths() {
        assert!(is_ephemeral_worktree(Path::new(
            r"C:\Users\x\repo\.claude\worktrees\agent-ab"
        )));
        assert!(!is_ephemeral_worktree(Path::new(
            r"C:\Users\x\repo"
        )));
    }
}
