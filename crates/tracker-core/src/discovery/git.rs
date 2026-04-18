//! Git-based enrichment for a project directory.

use std::path::Path;
use std::process::Command;

pub fn remote_origin(path: &Path) -> Option<String> {
    if !path.join(".git").exists() {
        return None;
    }
    run(path, &["remote", "get-url", "origin"])
        .and_then(|s| {
            let s = s.trim();
            if s.is_empty() {
                None
            } else {
                Some(normalize_github_url(s))
            }
        })
}

pub fn current_branch(path: &Path) -> Option<String> {
    run(path, &["rev-parse", "--abbrev-ref", "HEAD"]).map(|s| s.trim().to_string())
}

pub fn dirty(path: &Path) -> Option<bool> {
    run(path, &["status", "--porcelain"]).map(|s| !s.trim().is_empty())
}

fn run(path: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(path).args(args);
    let output = cmd.output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// Canonicalize a `git@github.com:owner/repo.git` to `https://github.com/owner/repo`.
pub fn normalize_github_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    let trimmed = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    if let Some(rest) = trimmed.strip_prefix("git@github.com:") {
        return format!("https://github.com/{rest}");
    }
    if let Some(rest) = trimmed.strip_prefix("ssh://git@github.com/") {
        return format!("https://github.com/{rest}");
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_ssh_url() {
        assert_eq!(
            normalize_github_url("git@github.com:owner/repo.git"),
            "https://github.com/owner/repo"
        );
    }

    #[test]
    fn normalize_https_url() {
        assert_eq!(
            normalize_github_url("https://github.com/owner/repo.git"),
            "https://github.com/owner/repo"
        );
    }

    #[test]
    fn normalize_ssh_scheme() {
        assert_eq!(
            normalize_github_url("ssh://git@github.com/owner/repo.git"),
            "https://github.com/owner/repo"
        );
    }
}
