//! Read recent-projects lists from IDE caches on disk. Every reader is
//! best-effort: if a cache is missing, locked, or in an unfamiliar format,
//! the scan just skips that source rather than propagating an error.

use std::path::{Path, PathBuf};

use regex::Regex;
use rusqlite::{Connection, OpenFlags};

/// One recent-projects source, e.g. VS Code or a specific JetBrains product.
pub struct IdeHit {
    pub path: PathBuf,
    pub source: String,
}

pub fn scan_all() -> Vec<IdeHit> {
    let mut hits = Vec::new();
    hits.extend(scan_vscode_family());
    hits.extend(scan_jetbrains());
    dedupe(hits)
}

fn dedupe(hits: Vec<IdeHit>) -> Vec<IdeHit> {
    let mut seen = std::collections::HashSet::new();
    hits.into_iter()
        .filter(|h| seen.insert(h.path.clone()))
        .collect()
}

// ---------- VS Code / Cursor / VS Code Insiders ----------

/// Product directories that share the VS Code on-disk layout.
const VSCODE_VARIANTS: &[(&str, &str)] = &[
    ("Code", "vscode"),
    ("Code - Insiders", "vscode-insiders"),
    ("Cursor", "cursor"),
];

fn scan_vscode_family() -> Vec<IdeHit> {
    let Some(cfg) = dirs::config_dir() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (dir, source) in VSCODE_VARIANTS {
        let global_storage = cfg.join(dir).join("User").join("globalStorage");
        for path in read_vscode_recent(&global_storage) {
            out.push(IdeHit {
                path,
                source: (*source).to_string(),
            });
        }
    }
    out
}

fn read_vscode_recent(global_storage: &Path) -> Vec<PathBuf> {
    // Newer VS Code (1.86+) stores recent paths in state.vscdb. Older
    // versions wrote storage.json. Try the DB first, fall back to the
    // legacy file.
    let vscdb = read_vscdb(&global_storage.join("state.vscdb"));
    if !vscdb.is_empty() {
        return vscdb;
    }
    read_storage_json(&global_storage.join("storage.json"))
}

fn read_vscdb(path: &Path) -> Vec<PathBuf> {
    let Ok(conn) = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY) else {
        return Vec::new();
    };
    let value: Option<String> = conn
        .prepare("SELECT value FROM ItemTable WHERE key = 'history.recentlyOpenedPathsList'")
        .ok()
        .and_then(|mut s| s.query_row([], |r| r.get::<_, String>(0)).ok());
    let Some(json) = value else { return Vec::new() };
    parse_vscode_entries(&json)
}

fn read_storage_json(path: &Path) -> Vec<PathBuf> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Vec::new();
    };
    let Some(entries) = v.pointer("/openedPathsList/entries").and_then(|e| e.as_array()) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|e| e.get("folderUri").and_then(|u| u.as_str()))
        .filter_map(decode_file_uri)
        .collect()
}

fn parse_vscode_entries(raw: &str) -> Vec<PathBuf> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) else {
        return Vec::new();
    };
    let Some(arr) = v.get("entries").and_then(|e| e.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|e| e.get("folderUri").and_then(|u| u.as_str()))
        .filter_map(decode_file_uri)
        .collect()
}

/// `file:///Users/foo/proj` → `/Users/foo/proj`.
/// `file:///C:/Users/foo/proj` → `C:/Users/foo/proj` on Windows.
fn decode_file_uri(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    // Windows drive paths arrive as `/C:/...` — strip the leading slash.
    let rest: String = if cfg!(windows)
        && rest.len() >= 3
        && rest.starts_with('/')
        && rest.as_bytes()[2] == b':'
    {
        rest[1..].to_string()
    } else {
        rest.to_string()
    };
    let decoded = percent_decode(&rest)?;
    Some(PathBuf::from(decoded))
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            let b = u8::from_str_radix(hex, 16).ok()?;
            out.push(b);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

// ---------- JetBrains ----------

fn scan_jetbrains() -> Vec<IdeHit> {
    let Some(cfg) = dirs::config_dir() else {
        return Vec::new();
    };
    let root = cfg.join("JetBrains");
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let home = dirs::home_dir();
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let product_dir = entry.path();
        let xml = product_dir.join("options").join("recentProjects.xml");
        let Ok(raw) = std::fs::read_to_string(&xml) else {
            continue;
        };
        let product = product_dir
            .file_name()
            .and_then(|n| n.to_str())
            .map(jetbrains_product_slug)
            .unwrap_or_else(|| "jetbrains".to_string());
        for path in parse_jetbrains_xml(&raw, home.as_deref()) {
            out.push(IdeHit {
                path,
                source: product.clone(),
            });
        }
    }
    out
}

/// Strip the trailing version suffix from a JetBrains directory name so the
/// UI shows "IntelliJIdea" rather than "IntelliJIdea2024.3".
fn jetbrains_product_slug(dir_name: &str) -> String {
    // Find where the version starts — first digit after at least one letter.
    let cut = dir_name
        .char_indices()
        .find(|(i, c)| c.is_ascii_digit() && *i > 0)
        .map(|(i, _)| i)
        .unwrap_or(dir_name.len());
    let base = &dir_name[..cut];
    format!("jetbrains-{}", base.to_ascii_lowercase())
}

fn parse_jetbrains_xml(xml: &str, home: Option<&Path>) -> Vec<PathBuf> {
    // Exact anchor: inside <component name="RecentProjectsManager"> the
    // project list is a <map> of <entry key="PATH"> children. The regex
    // is forgiving about whitespace but still path-shaped.
    let re = Regex::new(r#"<entry\s+key="([^"]+)""#).unwrap();
    let home_str = home.map(|h| h.to_string_lossy().into_owned());
    re.captures_iter(xml)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .map(|raw| match home_str.as_deref() {
            Some(home) => raw.replace("$USER_HOME$", home),
            None => raw,
        })
        // Entries whose path can't be resolved (unknown token) start with
        // `$`; drop those rather than treating them as literal paths.
        .filter(|p| !p.starts_with('$'))
        // The XML also contains non-project `<entry>` elements in other
        // sections. Path-shape filter keeps the signal high.
        .filter(|p| p.contains(std::path::MAIN_SEPARATOR) || p.contains('/') || p.contains('\\'))
        .map(PathBuf::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_simple_posix_uri() {
        let p = decode_file_uri("file:///Users/foo/my%20proj").unwrap();
        assert_eq!(p, PathBuf::from("/Users/foo/my proj"));
    }

    #[test]
    fn parse_vscode_entries_picks_folder_uris_only() {
        let raw = r#"{"entries":[
            {"folderUri":"file:///a/b"},
            {"fileUri":"file:///c/d.txt"},
            {"workspace":{"configPath":"file:///w.code-workspace"}},
            {"folderUri":"file:///e%20f/g"}
        ]}"#;
        let got = parse_vscode_entries(raw);
        assert_eq!(got, vec![PathBuf::from("/a/b"), PathBuf::from("/e f/g")]);
    }

    #[test]
    fn parse_jetbrains_extracts_entries_and_expands_home() {
        let home = Path::new("/home/artem");
        let xml = r#"<application>
          <component name="RecentProjectsManager">
            <option name="additionalInfo">
              <map>
                <entry key="$USER_HOME$/IdeaProjects/alpha">
                  <value><RecentProjectMetaInfo/></value>
                </entry>
                <entry key="/srv/absolute/beta">
                  <value><RecentProjectMetaInfo/></value>
                </entry>
                <entry key="$APPLICATION_HOME_DIR$/system">
                  <value><RecentProjectMetaInfo/></value>
                </entry>
              </map>
            </option>
          </component>
        </application>"#;
        let got = parse_jetbrains_xml(xml, Some(home));
        assert_eq!(
            got,
            vec![
                PathBuf::from("/home/artem/IdeaProjects/alpha"),
                PathBuf::from("/srv/absolute/beta"),
            ]
        );
    }

    #[test]
    fn jetbrains_product_slug_strips_version() {
        assert_eq!(jetbrains_product_slug("IntelliJIdea2024.3"), "jetbrains-intellijidea");
        assert_eq!(jetbrains_product_slug("PyCharm2023.2"), "jetbrains-pycharm");
        assert_eq!(jetbrains_product_slug("WebStorm"), "jetbrains-webstorm");
    }
}
