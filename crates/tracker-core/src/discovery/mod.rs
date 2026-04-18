pub mod claude_projects;
pub mod deploy;
pub mod git;
pub mod jsonl;
pub mod launch;

use std::path::PathBuf;

use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::db::Db;

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
