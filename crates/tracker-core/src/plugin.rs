//! Detect whether the `claude-tracker` Claude Code plugin is installed on
//! this machine. Plugin installs live under `~/.claude/plugins/` but the
//! exact path shape varies (marketplace cache subdirs, local dev symlinks,
//! etc.), so we walk a bounded depth and match on the plugin's `name`
//! field rather than hard-coding a path.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::paths;

const PLUGIN_NAME: &str = "claude-tracker";
const MAX_DEPTH: usize = 6;

#[derive(Debug, Clone, Serialize)]
pub struct PluginInstall {
    /// Root directory of the plugin (parent of `.claude-plugin/`).
    pub path: String,
    /// `version` field from the plugin manifest, if readable.
    pub version: Option<String>,
}

pub fn find_installed() -> Option<PluginInstall> {
    let root = paths::claude_dir().ok()?.join("plugins");
    if !root.exists() {
        return None;
    }
    for entry in walkdir::WalkDir::new(&root)
        .max_depth(MAX_DEPTH)
        .into_iter()
        .filter_map(Result::ok)
    {
        let p = entry.path();
        if p.file_name().and_then(|n| n.to_str()) != Some("plugin.json") {
            continue;
        }
        if !is_claude_plugin_manifest(p) {
            continue;
        }
        if let Some(install) = read_matching_manifest(p) {
            return Some(install);
        }
    }
    None
}

fn is_claude_plugin_manifest(path: &Path) -> bool {
    path.parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        == Some(".claude-plugin")
}

fn read_matching_manifest(manifest: &Path) -> Option<PluginInstall> {
    let raw = std::fs::read_to_string(manifest).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    if v.get("name").and_then(|n| n.as_str()) != Some(PLUGIN_NAME) {
        return None;
    }
    let plugin_root: PathBuf = manifest.parent()?.parent()?.to_path_buf();
    let version = v
        .get("version")
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());
    Some(PluginInstall {
        path: plugin_root.to_string_lossy().into_owned(),
        version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_claude_plugin_manifest_checks_parent_dir() {
        assert!(is_claude_plugin_manifest(Path::new(
            "/tmp/something/.claude-plugin/plugin.json"
        )));
        assert!(!is_claude_plugin_manifest(Path::new(
            "/tmp/something/plugin.json"
        )));
        assert!(!is_claude_plugin_manifest(Path::new(
            "/tmp/something/other-dir/plugin.json"
        )));
    }

    #[test]
    fn read_matching_manifest_returns_none_for_other_plugins() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join(".claude-plugin/plugin.json");
        std::fs::create_dir_all(manifest.parent().unwrap()).unwrap();
        std::fs::write(
            &manifest,
            r#"{"name":"some-other-plugin","version":"1.0.0"}"#,
        )
        .unwrap();
        assert!(read_matching_manifest(&manifest).is_none());
    }

    #[test]
    fn read_matching_manifest_picks_up_claude_tracker() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join(".claude-plugin/plugin.json");
        std::fs::create_dir_all(manifest.parent().unwrap()).unwrap();
        std::fs::write(
            &manifest,
            r#"{"name":"claude-tracker","version":"0.2.1"}"#,
        )
        .unwrap();
        let got = read_matching_manifest(&manifest).unwrap();
        assert_eq!(got.version.as_deref(), Some("0.2.1"));
        assert_eq!(got.path, dir.path().to_string_lossy());
    }
}
