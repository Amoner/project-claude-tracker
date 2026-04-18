//! Infer the "how do I run this locally?" command from the most common
//! project-manifest signals.

use std::path::Path;

use serde::Deserialize;

/// Return a multi-line string with one or more commands, most-preferred first.
pub fn infer(path: &Path) -> Option<String> {
    let (cmd, _) = analyze(path);
    cmd
}

pub fn infer_name(path: &Path) -> Option<String> {
    let (_, name) = analyze(path);
    name.or_else(|| {
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
    })
}

/// Walk through each known manifest once, collecting the best launch command
/// and the best project name. Each manifest is read+parsed at most once.
fn analyze(path: &Path) -> (Option<String>, Option<String>) {
    let mut cmds: Vec<String> = Vec::new();
    let mut name: Option<String> = None;

    let (c, n) = from_package_json(path);
    push_if(&mut cmds, c);
    name = name.or(n);

    let (c, n) = from_pyproject(path);
    push_if(&mut cmds, c);
    name = name.or(n);

    push_if(&mut cmds, from_task_runner(path, "Makefile", "make"));
    push_if(&mut cmds, from_task_runner(path, "justfile", "just"));

    let (c, n) = from_cargo_toml(path);
    push_if(&mut cmds, c);
    name = name.or(n);

    let launch = if cmds.is_empty() {
        None
    } else {
        let mut seen = std::collections::HashSet::new();
        let joined: Vec<String> = cmds
            .into_iter()
            .filter(|s| seen.insert(s.clone()))
            .collect();
        Some(joined.join("\n"))
    };
    (launch, name)
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
    scripts: std::collections::BTreeMap<String, String>,
}

fn from_package_json(path: &Path) -> (Option<String>, Option<String>) {
    let Ok(contents) = std::fs::read_to_string(path.join("package.json")) else {
        return (None, None);
    };
    let Ok(pkg) = serde_json::from_str::<PackageJson>(&contents) else {
        return (None, None);
    };
    let manager = detect_node_pkg_manager(path);
    let cmd = ["dev", "start"]
        .iter()
        .find(|k| pkg.scripts.contains_key(**k))
        .map(|k| format!("{manager} run {k}"));
    (cmd, pkg.name)
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
    scripts: std::collections::BTreeMap<String, String>,
}

fn from_pyproject(path: &Path) -> (Option<String>, Option<String>) {
    let Ok(contents) = std::fs::read_to_string(path.join("pyproject.toml")) else {
        return (None, None);
    };
    let Ok(parsed) = toml::from_str::<Pyproject>(&contents) else {
        return (None, None);
    };
    let name = parsed.project.as_ref().and_then(|p| p.name.clone());
    let scripts = parsed
        .project
        .as_ref()
        .map(|p| p.scripts.clone())
        .unwrap_or_default();
    let poetry_scripts = parsed
        .tool
        .and_then(|t| t.poetry)
        .map(|p| p.scripts)
        .unwrap_or_default();
    for key in ["dev", "start", "run", "serve"] {
        if let Some(v) = scripts.get(key) {
            return (Some(v.clone()), name);
        }
        if let Some(v) = poetry_scripts.get(key) {
            return (Some(format!("poetry run {v}")), name);
        }
    }
    (None, name)
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
}

#[derive(Deserialize)]
struct CargoBin {
    #[serde(default)]
    name: Option<String>,
}

fn from_cargo_toml(path: &Path) -> (Option<String>, Option<String>) {
    let Ok(contents) = std::fs::read_to_string(path.join("Cargo.toml")) else {
        return (None, None);
    };
    let Ok(parsed) = toml::from_str::<CargoToml>(&contents) else {
        return (None, None);
    };
    let cmd = if let Some(bin) = parsed.bin.first().and_then(|b| b.name.as_ref()) {
        Some(format!("cargo run --bin {bin}"))
    } else if parsed.package.is_some() {
        Some("cargo run".to_string())
    } else {
        None
    };
    let name = parsed.package.and_then(|p| p.name);
    (cmd, name)
}
