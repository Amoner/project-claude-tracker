pub mod claude_projects;
pub mod deploy;
pub mod filesystem;
pub mod git;
pub mod ide;
pub mod jsonl;
pub mod launch;

use std::path::PathBuf;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::db::Db;
use crate::paths;

/// A project surfaced by scanning `~/.claude/projects/`.
#[derive(Debug, Clone, Default)]
pub struct DiscoveredProject {
    pub path: PathBuf,
    pub name: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_active_at: Option<DateTime<Utc>>,
    pub sessions_started: i64,
    pub prompts_count: i64,
}

/// Aggregated enrichment results for a single project directory.
#[derive(Debug, Clone, Default)]
pub struct EnrichmentResult {
    pub github_url: Option<String>,
    pub launch_instructions: Option<String>,
    pub deploy_platform: Option<String>,
    pub deploy_url: Option<String>,
    pub name: Option<String>,
}

/// A project suggested by the IDE-cache / filesystem walk / manual add
/// flows. `already_tracked` lets the UI dim rows the DB already knows
/// about so the user can focus on genuinely new candidates.
#[derive(Debug, Clone, Serialize)]
pub struct ScanCandidate {
    pub path: String,
    pub name: String,
    pub source: String,
    pub already_tracked: bool,
}

/// Scan `~/.claude/projects/` and upsert every project into the DB inside a
/// single transaction. Returns `(total_seen, newly_added)`.
pub fn discover_all(db: &Db) -> Result<(usize, usize)> {
    let discovered = claude_projects::scan_all()?;
    let added = db.with_tx(|db| {
        let mut added = 0usize;
        for dp in &discovered {
            if db.find_id_by_path(&dp.path)?.is_none() {
                added += 1;
            }
            db.seed_project(dp)?;
        }
        Ok(added)
    })?;
    Ok((discovered.len(), added))
}

/// Walk the given roots (tilde-expanded) for git repos and return them as
/// scan candidates. Never modifies the DB.
pub fn scan_filesystem(db: &Db, roots: &[String], max_depth: usize) -> Result<Vec<ScanCandidate>> {
    let resolved: Vec<PathBuf> = roots.iter().map(|r| paths::expand_tilde(r)).collect();
    let hits = filesystem::scan_roots(&resolved, max_depth);
    hits.into_iter()
        .map(|p| candidate_from_path(db, p, "fs-walk"))
        .collect()
}

/// Read recent-project lists from supported IDEs and return them as scan
/// candidates. Never modifies the DB.
pub fn scan_ide(db: &Db) -> Result<Vec<ScanCandidate>> {
    ide::scan_all()
        .into_iter()
        .map(|h| candidate_from_path(db, h.path, &h.source))
        .collect()
}

fn candidate_from_path(db: &Db, path: PathBuf, source: &str) -> Result<ScanCandidate> {
    let name = paths::project_name_from_path(&path);
    let already_tracked = db.find_id_by_path(&path)?.is_some();
    Ok(ScanCandidate {
        path: path.to_string_lossy().into_owned(),
        name,
        source: source.to_string(),
        already_tracked,
    })
}

/// Bulk-insert paths into the DB. Returns the number of NEW rows created
/// (paths already tracked are skipped silently). Runs inside one tx.
pub fn import_paths(db: &Db, paths_in: &[String]) -> Result<usize> {
    db.with_tx(|db| {
        let mut added = 0usize;
        for raw in paths_in {
            let path = paths::expand_tilde(raw);
            if db.find_id_by_path(&path)?.is_some() {
                continue;
            }
            let name = paths::project_name_from_path(&path);
            db.upsert_project_by_path(&path, &name)?;
            added += 1;
        }
        Ok(added)
    })
}

/// Add a single project by explicit path. Returns the path that was
/// upserted, tilde-expanded and canonicalized if possible.
pub fn add_manual(db: &Db, raw_path: &str) -> Result<PathBuf> {
    let expanded = paths::expand_tilde(raw_path);
    let canonical = std::fs::canonicalize(&expanded).unwrap_or(expanded);
    if !canonical.is_dir() {
        anyhow::bail!("not a directory: {}", canonical.display());
    }
    let name = paths::project_name_from_path(&canonical);
    db.upsert_project_by_path(&canonical, &name)?;
    Ok(canonical)
}

