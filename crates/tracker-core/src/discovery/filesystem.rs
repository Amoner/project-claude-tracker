//! Bounded filesystem walk: for each configured root, find git repos and
//! surface them as candidates. Limited by depth and a skiplist of heavy
//! subdirectories so scanning a messy `$HOME` stays responsive.

use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// Directory names we never want to descend into. Covers the most common
/// build outputs and virtualenvs across ecosystems. These are cheap to
/// check — match-on-name, no stat required.
const IGNORE_NAMES: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    ".venv",
    "venv",
    "env",
    "__pycache__",
    "dist",
    "build",
    ".next",
    ".nuxt",
    ".turbo",
    ".cache",
    ".gradle",
    ".idea",
    ".vscode",
    "vendor",
    "Pods",
    ".DS_Store",
];

/// Walk `root` up to `max_depth` deep, returning the path of every
/// directory that contains a `.git/` child. When a git repo is found the
/// walker stops descending further into it (submodules get skipped too —
/// that's an acceptable tradeoff for v1).
pub fn scan_root(root: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !root.is_dir() {
        return out;
    }
    let mut it = WalkDir::new(root)
        .max_depth(max_depth)
        .follow_links(false)
        .into_iter();
    loop {
        let Some(entry) = it.next() else { break };
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        if IGNORE_NAMES.iter().any(|n| *n == name.as_ref()) {
            it.skip_current_dir();
            continue;
        }
        if entry.path().join(".git").exists() {
            out.push(entry.path().to_path_buf());
            it.skip_current_dir();
        }
    }
    out
}

/// Scan every root and return a deduplicated list of git repos in
/// discovery order (first occurrence wins).
pub fn scan_roots(roots: &[PathBuf], max_depth: usize) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for root in roots {
        for repo in scan_root(root, max_depth) {
            if seen.insert(repo.clone()) {
                out.push(repo);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_git_repo_and_stops_descending() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let repo = root.join("proj");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join("src/nested")).unwrap();

        let found = scan_root(root, 5);
        assert_eq!(found, vec![repo.clone()]);
    }

    #[test]
    fn skips_node_modules_and_target() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let repo = root.join("proj");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let hidden = repo.join("node_modules/pkg");
        std::fs::create_dir_all(hidden.join(".git")).unwrap();

        let found = scan_root(root, 6);
        // The outer project is found; descent stops, so the inner fake repo
        // inside node_modules is never visited.
        assert_eq!(found, vec![repo]);
    }

    #[test]
    fn respects_depth_cap() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let deep = root.join("a/b/c/d/e/proj");
        std::fs::create_dir_all(deep.join(".git")).unwrap();

        assert!(scan_root(root, 3).is_empty());
        assert_eq!(scan_root(root, 7), vec![deep]);
    }

    #[test]
    fn nonexistent_root_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist");
        assert!(scan_root(&missing, 5).is_empty());
    }

    #[test]
    fn scan_roots_dedupes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let repo = root.join("proj");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        let got = scan_roots(&[root.to_path_buf(), root.to_path_buf()], 3);
        assert_eq!(got, vec![repo]);
    }
}
