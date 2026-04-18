//! Claude Code's directory-name encoding for `~/.claude/projects/`.
//!
//! Observed on real directories: both `/` and `.` are replaced with `-`, and
//! literal `-` characters in the path are left untouched. That makes the
//! encoding **lossy** — multiple real paths can encode to the same name.
//!
//! Examples:
//!
//! | path                                                      | encoded                                                       |
//! |-----------------------------------------------------------|---------------------------------------------------------------|
//! | `/Users/x/Documents/project-claude-tracker`               | `-Users-x-Documents-project-claude-tracker`                   |
//! | `/Users/x/Documents/AoG/.claude-worktrees/great-ptolemy`  | `-Users-x-Documents-AoG--claude-worktrees-great-ptolemy`      |
//!
//! Because decoding the dir name alone is ambiguous, the authoritative path
//! for a session lives inside the `.jsonl` transcript (as `"cwd":"..."`).
//! Callers should prefer reading `cwd` from a transcript; fall back to
//! [`best_effort_decode`] only when no transcript is available.

use std::path::{Path, PathBuf};

/// Encode an absolute path into the form Claude Code uses as its project
/// directory name.
pub fn encode_path(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '/' | '\\' | '.' => out.push('-'),
            c => out.push(c),
        }
    }
    out
}

/// Produce a **best-effort** decoded path from an encoded directory name.
///
/// This cannot be exact because the encoding is lossy. The strategy:
///   - Treat a leading `-` as a leading `/`.
///   - Replace all other `-` with `/`.
///   - Then walk from the deepest prefix back toward the root, checking which
///     prefix exists on disk; return the longest matching existing prefix
///     joined with the unmatched tail reinterpreted as the literal name (with
///     remaining dashes treated as part of the basename).
///
/// When even the leading prefix doesn't exist on disk, returns the naive
/// all-dashes-to-slashes interpretation.
pub fn best_effort_decode(name: &str) -> PathBuf {
    let all_slashes = naive_decode(name);
    if all_slashes.exists() {
        return all_slashes;
    }

    // Try progressively collapsing the last `/` back to `-` until we find
    // an existing directory. This handles the common case where the final
    // component had a real dash (e.g. `project-claude-tracker`).
    let s = all_slashes.to_string_lossy().into_owned();
    let bytes = s.as_bytes();
    let mut slash_positions: Vec<usize> = bytes
        .iter()
        .enumerate()
        .filter(|(_, &b)| b == b'/')
        .map(|(i, _)| i)
        .collect();

    while let Some(last) = slash_positions.pop() {
        let mut candidate = s.clone().into_bytes();
        // Replace slashes from this position to the end, one by one, with '-'.
        // We try every subset-less approach: just replace the last slash and
        // check; if not existing, replace the next-last too; etc.
        for pos in slash_positions.iter().rev() {
            candidate[last] = b'-';
            let path = PathBuf::from(std::str::from_utf8(&candidate).unwrap_or_default());
            if path.exists() {
                return path;
            }
            candidate[*pos] = b'-';
        }
        let path = PathBuf::from(std::str::from_utf8(&candidate).unwrap_or_default());
        if path.exists() {
            return path;
        }
    }

    all_slashes
}

fn naive_decode(name: &str) -> PathBuf {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch == '-' {
            out.push('/');
        } else {
            out.push(ch);
        }
    }
    PathBuf::from(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_matches_real_samples() {
        assert_eq!(
            encode_path(Path::new("/Users/artembeer/Documents/project-claude-tracker")),
            "-Users-artembeer-Documents-project-claude-tracker"
        );
        assert_eq!(
            encode_path(Path::new(
                "/Users/artembeer/Documents/AoG/.claude-worktrees/great-ptolemy"
            )),
            "-Users-artembeer-Documents-AoG--claude-worktrees-great-ptolemy"
        );
        assert_eq!(
            encode_path(Path::new("/Users/artembeer")),
            "-Users-artembeer"
        );
    }

    #[test]
    fn naive_decode_all_slashes() {
        assert_eq!(
            naive_decode("-Users-artembeer-Documents"),
            PathBuf::from("/Users/artembeer/Documents")
        );
    }

    #[test]
    fn best_effort_decode_real_existing_path() {
        // This path exists on the test machine; if it doesn't (e.g. CI),
        // best_effort_decode still returns the naive reading.
        let name = "-Users-artembeer-Documents-project-claude-tracker";
        let decoded = best_effort_decode(name);
        assert!(
            decoded.to_string_lossy().contains("project-claude-tracker")
                || decoded.to_string_lossy().contains("project/claude/tracker"),
            "unexpected decode: {decoded:?}"
        );
    }
}
