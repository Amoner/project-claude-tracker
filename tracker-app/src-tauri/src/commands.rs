use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::State;
use tracker_core::db::{Project, ProjectUpdate};
use tracker_core::discovery::ScanCandidate;
use tracker_core::status::{self, StatusInputs};
use tracker_core::terminal::Terminal;
use tracker_core::{discovery, hooks, os, paths, sync};

use crate::AppState;

type Shared<'a> = State<'a, Arc<AppState>>;

fn err<E: std::fmt::Display>(e: E) -> String {
    format!("{e:#}")
}

#[derive(Serialize)]
pub struct ProjectDto {
    #[serde(flatten)]
    project: Project,
    effective_status: String,
}

impl From<Project> for ProjectDto {
    fn from(p: Project) -> Self {
        let effective = effective_status(&p);
        Self {
            project: p,
            effective_status: effective,
        }
    }
}

fn effective_status(p: &Project) -> String {
    // User-set override wins, but only when both the manual flag AND a
    // non-empty status string are present.
    if p.status_manual {
        if let Some(s) = p.status.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            return s.to_string();
        }
    }
    status::infer(&StatusInputs {
        last_active_at: p.last_active_at,
        deploy_url: p.deploy_url.as_deref(),
        archived_at: p.archived_at,
        now: Utc::now(),
    })
    .as_str()
    .to_string()
}

#[derive(Serialize)]
pub struct HookStatusDto {
    pub settings_path: String,
    pub installed_events: Vec<String>,
    pub cli_path: Option<String>,
    pub fully_installed: bool,
}

impl From<hooks::HookStatus> for HookStatusDto {
    fn from(s: hooks::HookStatus) -> Self {
        let fully = hooks::INSTALLED_EVENTS
            .iter()
            .all(|e| s.installed_events.iter().any(|i| i == *e));
        Self {
            settings_path: s.settings_path.to_string_lossy().into_owned(),
            installed_events: s.installed_events,
            cli_path: s.cli_path.map(|p| p.to_string_lossy().into_owned()),
            fully_installed: fully,
        }
    }
}

#[tauri::command]
pub fn list_projects(
    state: Shared<'_>,
    include_archived: bool,
) -> Result<Vec<ProjectDto>, String> {
    let db = state.db.lock().map_err(err)?;
    Ok(db
        .list_projects(include_archived)
        .map_err(err)?
        .into_iter()
        .map(ProjectDto::from)
        .collect())
}

#[tauri::command]
pub fn get_project(state: Shared<'_>, id: i64) -> Result<Option<ProjectDto>, String> {
    let db = state.db.lock().map_err(err)?;
    Ok(db.get_project(id).map_err(err)?.map(ProjectDto::from))
}

#[tauri::command]
pub fn recent_active(state: Shared<'_>, limit: usize) -> Result<Vec<ProjectDto>, String> {
    let db = state.db.lock().map_err(err)?;
    Ok(db
        .recent_active(limit)
        .map_err(err)?
        .into_iter()
        .map(ProjectDto::from)
        .collect())
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateFieldsDto {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub status_manual: Option<bool>,
    #[serde(default)]
    pub deploy_url: Option<String>,
    #[serde(default)]
    pub deploy_instructions: Option<String>,
    #[serde(default)]
    pub launch_instructions: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub deploy_live_lookup: Option<bool>,
    #[serde(default)]
    pub archived: Option<bool>,
}

#[tauri::command]
pub fn update_project(
    state: Shared<'_>,
    id: i64,
    fields: UpdateFieldsDto,
) -> Result<ProjectDto, String> {
    let db = state.db.lock().map_err(err)?;
    let update = ProjectUpdate {
        name: fields.name,
        status: fields.status,
        status_manual: fields.status_manual,
        deploy_url: fields.deploy_url,
        deploy_instructions: fields.deploy_instructions,
        launch_instructions: fields.launch_instructions,
        notes: fields.notes,
        deploy_live_lookup: fields.deploy_live_lookup,
        archived: fields.archived,
        ..Default::default()
    };
    db.update_project_fields(id, &update).map_err(err)?;
    db.get_project(id)
        .map_err(err)?
        .map(ProjectDto::from)
        .ok_or_else(|| "project not found after update".into())
}

#[tauri::command]
pub fn run_sync(state: Shared<'_>, force: bool, live_lookup: bool) -> Result<usize, String> {
    let db = state.db.lock().map_err(err)?;
    sync::sync_all(
        &db,
        &sync::SyncOpts {
            force,
            allow_live_lookup: live_lookup,
        },
    )
    .map_err(err)
}

#[tauri::command]
pub fn run_discover(state: Shared<'_>) -> Result<usize, String> {
    let db = state.db.lock().map_err(err)?;
    discovery::discover_all(&db).map(|(_, added)| added).map_err(err)
}

#[tauri::command]
pub fn get_hook_status() -> Result<HookStatusDto, String> {
    hooks::status().map(Into::into).map_err(err)
}

#[tauri::command]
pub fn install_hooks() -> Result<HookStatusDto, String> {
    // Hook scripts must point at a concrete tracker-cli binary. We ship one
    // alongside the app bundle; dev mode falls back to the workspace target.
    let cli_path = tracker_cli_path().ok_or_else(|| {
        "could not locate tracker-cli binary; build it with `cargo build -p tracker-cli` first"
            .to_string()
    })?;
    hooks::install(&cli_path).map(Into::into).map_err(err)
}

#[tauri::command]
pub fn uninstall_hooks() -> Result<HookStatusDto, String> {
    hooks::uninstall().map(Into::into).map_err(err)
}

#[tauri::command]
pub fn open_in_finder(path: String) -> Result<(), String> {
    os::open_path(std::path::Path::new(&path)).map_err(err)
}

#[derive(Serialize)]
pub struct TerminalInfoDto {
    pub slug: String,
    pub display_name: String,
    pub installed: bool,
}

const PREFERRED_TERMINAL_KEY: &str = "preferred_terminal";
const SEEN_VERSION_KEY: &str = "seen_version";

#[tauri::command]
pub fn list_terminals() -> Vec<TerminalInfoDto> {
    Terminal::priority()
        .iter()
        .map(|t| TerminalInfoDto {
            slug: t.slug().to_string(),
            display_name: t.display_name().to_string(),
            installed: t.is_installed(),
        })
        .collect()
}

#[tauri::command]
pub fn get_preferred_terminal(state: Shared<'_>) -> Result<Option<String>, String> {
    let db = state.db.lock().map_err(err)?;
    db.get_setting(PREFERRED_TERMINAL_KEY).map_err(err)
}

#[tauri::command]
pub fn set_preferred_terminal(state: Shared<'_>, terminal: String) -> Result<(), String> {
    if Terminal::from_slug(&terminal).is_none() {
        return Err(format!("unknown terminal: {terminal}"));
    }
    let db = state.db.lock().map_err(err)?;
    db.set_setting(PREFERRED_TERMINAL_KEY, &terminal).map_err(err)
}

#[tauri::command]
pub fn start_claude(state: Shared<'_>, id: i64) -> Result<(), String> {
    // Resolve path + terminal under the lock, then drop it before spawning.
    let (path, terminal) = {
        let db = state.db.lock().map_err(err)?;
        let project = db
            .get_project(id)
            .map_err(err)?
            .ok_or_else(|| "project not found".to_string())?;
        let pref = db
            .get_setting(PREFERRED_TERMINAL_KEY)
            .map_err(err)?
            .and_then(|s| Terminal::from_slug(&s));
        let terminal = pref.unwrap_or_else(Terminal::default_installed);
        (project.path_buf(), terminal)
    };
    if !path.exists() {
        return Err(format!("project path does not exist: {}", path.display()));
    }
    terminal.launch(&path, "claude").map_err(|e| {
        paths::append_log("terminal.log", &format!("start_claude err: {e:#}"));
        err(e)
    })
}

#[tauri::command]
pub fn check_release_notes(
    state: Shared<'_>,
    app_handle: tauri::AppHandle,
) -> Result<Option<String>, String> {
    let version = app_handle.package_info().version.to_string();
    let db = state.db.lock().map_err(err)?;
    let seen = db.get_setting(SEEN_VERSION_KEY).map_err(err)?;
    if seen.as_deref() == Some(version.as_str()) {
        return Ok(None);
    }
    db.set_setting(SEEN_VERSION_KEY, &version).map_err(err)?;
    Ok(Some(version))
}

#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    os::open_url(&url).map_err(err)
}

#[tauri::command]
pub fn scan_ide_projects(state: Shared<'_>) -> Result<Vec<ScanCandidate>, String> {
    let db = state.db.lock().map_err(err)?;
    discovery::scan_ide(&db).map_err(err)
}

#[tauri::command]
pub fn scan_filesystem(
    state: Shared<'_>,
    roots: Vec<String>,
    max_depth: usize,
) -> Result<Vec<ScanCandidate>, String> {
    let db = state.db.lock().map_err(err)?;
    discovery::scan_filesystem(&db, &roots, max_depth).map_err(err)
}

#[tauri::command]
pub fn import_projects(state: Shared<'_>, paths: Vec<String>) -> Result<usize, String> {
    let db = state.db.lock().map_err(err)?;
    discovery::import_paths(&db, &paths).map_err(err)
}

#[tauri::command]
pub fn add_project_manual(state: Shared<'_>, path: String) -> Result<ProjectDto, String> {
    let db = state.db.lock().map_err(err)?;
    let added = discovery::add_manual(&db, &path).map_err(err)?;
    db.get_project_by_path(&added)
        .map_err(err)?
        .map(ProjectDto::from)
        .ok_or_else(|| "project not found after add".into())
}

/// Locate the tracker-cli binary.
///
/// Search order:
/// 1. Sibling of the current app binary (bundled release location).
/// 2. Workspace target/debug (when running `cargo tauri dev`).
/// 3. Workspace target/release (after a full release build).
fn tracker_cli_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    if let Some(dir) = exe.parent() {
        let sibling = dir.join(bin_name("tracker-cli"));
        if sibling.exists() {
            return Some(sibling);
        }
    }
    // Walk up looking for a workspace root with `target/`.
    let mut cur: Option<&std::path::Path> = Some(exe.as_path());
    while let Some(p) = cur {
        let target = p.join("target");
        if target.join("debug").join(bin_name("tracker-cli")).exists() {
            return Some(target.join("debug").join(bin_name("tracker-cli")));
        }
        if target.join("release").join(bin_name("tracker-cli")).exists() {
            return Some(target.join("release").join(bin_name("tracker-cli")));
        }
        cur = p.parent();
    }
    None
}

fn bin_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}
