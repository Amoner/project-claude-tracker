//! Infer the "how do I run this locally?" command, the project name, and a
//! one-line description from the most common project-manifest signals.

use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Default)]
struct ManifestInfo {
    cmd: Option<String>,
    name: Option<String>,
    description: Option<String>,
}

/// Return a multi-line string with one or more commands, most-preferred first.
pub fn infer(path: &Path) -> Option<String> {
    analyze(path).cmd
}

pub fn infer_name(path: &Path) -> Option<String> {
    analyze(path).name.or_else(|| {
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
    })
}

pub fn infer_description(path: &Path) -> Option<String> {
    analyze(path).description.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Walk through each known manifest once, collecting the best launch
/// command, project name, and description. Each manifest is read+parsed
/// at most once.
fn analyze(path: &Path) -> ManifestInfo {
    let mut info = ManifestInfo::default();
    let mut cmds: Vec<String> = Vec::new();

    let pkg = from_package_json(path);
    push_if(&mut cmds, pkg.cmd);
    info.name = info.name.or(pkg.name);
    info.description = info.description.or(pkg.description);

    let py = from_pyproject(path);
    push_if(&mut cmds, py.cmd);
    info.name = info.name.or(py.name);
    info.description = info.description.or(py.description);

    push_if(&mut cmds, from_task_runner(path, "Makefile", "make"));
    push_if(&mut cmds, from_task_runner(path, "justfile", "just"));

    let cargo = from_cargo_toml(path);
    push_if(&mut cmds, cargo.cmd);
    info.name = info.name.or(cargo.name);
    info.description = info.description.or(cargo.description);

    info.cmd = if cmds.is_empty() {
        None
    } else {
        let mut seen = std::collections::HashSet::new();
        Some(
            cmds.into_iter()
                .filter(|s| seen.insert(s.clone()))
                .collect::<Vec<_>>()
                .join("\n"),
        )
    };
    info
}

fn push_if(cmds: &mut Vec<String>, s: Option<String>) {
    if let Some(v) = s {
        cmds.push(v);
    }
}

#[derive(Deserialize)]
struct PackageJson {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    scripts: std::collections::BTreeMap<String, String>,
}

fn from_package_json(path: &Path) -> ManifestInfo {
    let Ok(contents) = std::fs::read_to_string(path.join("package.json")) else {
        return ManifestInfo::default();
    };
    let Ok(pkg) = serde_json::from_str::<PackageJson>(&contents) else {
        return ManifestInfo::default();
    };
    let manager = detect_node_pkg_manager(path);
    let cmd = ["dev", "start"]
        .iter()
        .find(|k| pkg.scripts.contains_key(**k))
        .map(|k| format!("{manager} run {k}"));
    ManifestInfo {
        cmd,
        name: pkg.name,
        description: pkg.description,
    }
}

fn detect_node_pkg_manager(path: &Path) -> &'static str {
    if path.join("pnpm-lock.yaml").exists() {
        "pnpm"
    } else if path.join("yarn.lock").exists() {
        "yarn"
    } else if path.join("bun.lockb").exists() {
        "bun"
    } else {
        "npm"
    }
}

#[derive(Deserialize)]
struct Pyproject {
    #[serde(default)]
    project: Option<PyProject>,
    #[serde(default)]
    tool: Option<PyTool>,
}

#[derive(Deserialize)]
struct PyProject {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    scripts: std::collections::BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct PyTool {
    #[serde(default)]
    poetry: Option<PoetryTool>,
}

#[derive(Deserialize)]
struct PoetryTool {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    scripts: std::collections::BTreeMap<String, String>,
}

fn from_pyproject(path: &Path) -> ManifestInfo {
    let Ok(contents) = std::fs::read_to_string(path.join("pyproject.toml")) else {
        return ManifestInfo::default();
    };
    let Ok(parsed) = toml::from_str::<Pyproject>(&contents) else {
        return ManifestInfo::default();
    };
    let name = parsed
        .project
        .as_ref()
        .and_then(|p| p.name.clone())
        .or_else(|| {
            parsed
                .tool
                .as_ref()
                .and_then(|t| t.poetry.as_ref())
                .and_then(|p| p.name.clone())
        });
    let description = parsed
        .project
        .as_ref()
        .and_then(|p| p.description.clone())
        .or_else(|| {
            parsed
                .tool
                .as_ref()
                .and_then(|t| t.poetry.as_ref())
                .and_then(|p| p.description.clone())
        });
    let scripts = parsed
        .project
        .map(|p| p.scripts)
        .unwrap_or_default();
    let poetry_scripts = parsed
        .tool
        .and_then(|t| t.poetry)
        .map(|p| p.scripts)
        .unwrap_or_default();
    let mut cmd = None;
    for key in ["dev", "start", "run", "serve"] {
        if let Some(v) = scripts.get(key) {
            cmd = Some(v.clone());
            break;
        }
        if let Some(v) = poetry_scripts.get(key) {
            cmd = Some(format!("poetry run {v}"));
            break;
        }
    }
    ManifestInfo {
        cmd,
        name,
        description,
    }
}

fn from_task_runner(path: &Path, filename: &str, prefix: &str) -> Option<String> {
    let raw = std::fs::read_to_string(path.join(filename)).ok()?;
    for target in ["dev", "run", "start", "serve"] {
        if raw.contains(&format!("\n{target}:")) || raw.starts_with(&format!("{target}:")) {
            return Some(format!("{prefix} {target}"));
        }
    }
    None
}

#[derive(Deserialize)]
struct CargoToml {
    #[serde(default)]
    package: Option<CargoPackage>,
    #[serde(default)]
    bin: Vec<CargoBin>,
}

#[derive(Deserialize)]
struct CargoPackage {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize)]
struct CargoBin {
    #[serde(default)]
    name: Option<String>,
}

fn from_cargo_toml(path: &Path) -> ManifestInfo {
    let Ok(contents) = std::fs::read_to_string(path.join("Cargo.toml")) else {
        return ManifestInfo::default();
    };
    let Ok(parsed) = toml::from_str::<CargoToml>(&contents) else {
        return ManifestInfo::default();
    };
    let cmd = if let Some(bin) = parsed.bin.first().and_then(|b| b.name.as_ref()) {
        Some(format!("cargo run --bin {bin}"))
    } else if parsed.package.is_some() {
        Some("cargo run".to_string())
    } else {
        None
    };
    let name = parsed.package.as_ref().and_then(|p| p.name.clone());
    let description = parsed.package.and_then(|p| p.description);
    ManifestInfo {
        cmd,
        name,
        description,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_description_from_package_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"demo","description":"a thing","scripts":{"dev":"next dev"}}"#,
        )
        .unwrap();
        let info = analyze(dir.path());
        assert_eq!(info.name.as_deref(), Some("demo"));
        assert_eq!(info.description.as_deref(), Some("a thing"));
        assert_eq!(info.cmd.as_deref(), Some("npm run dev"));
    }

    #[test]
    fn extracts_description_from_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            r#"[package]
name = "rusty"
description = "blazingly fast"
version = "0.1.0"
"#,
        )
        .unwrap();
        let info = analyze(dir.path());
        assert_eq!(info.name.as_deref(), Some("rusty"));
        assert_eq!(info.description.as_deref(), Some("blazingly fast"));
    }

    #[test]
    fn extracts_description_from_pyproject_poetry() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("pyproject.toml"),
            r#"[tool.poetry]
name = "snek"
description = "pythonic things"
"#,
        )
        .unwrap();
        let info = analyze(dir.path());
        assert_eq!(info.name.as_deref(), Some("snek"));
        assert_eq!(info.description.as_deref(), Some("pythonic things"));
    }

    #[test]
    fn infer_description_trims_and_rejects_empty() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"x","description":"   "}"#,
        )
        .unwrap();
        assert!(infer_description(dir.path()).is_none());
    }
}
