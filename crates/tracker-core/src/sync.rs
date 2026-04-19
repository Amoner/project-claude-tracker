//! Enrichment worker: for each known project, refresh facts that can be
//! derived from disk (git remote, launch instructions, deploy platform) and
//! optionally shell out to deploy CLIs for live URLs.

use std::path::Path;

use anyhow::Result;
use chrono::{Duration, Utc};

use crate::db::{Db, Project, ProjectUpdate};
use crate::discovery::{deploy, git, launch, EnrichmentResult};

const STALE_AFTER: Duration = Duration::hours(1);

#[derive(Debug, Default)]
pub struct SyncOpts {
    /// Ignore the per-project enrichment_synced_at cache.
    pub force: bool,
    /// Respect per-project deploy_live_lookup flag.
    pub allow_live_lookup: bool,
}

pub fn sync_all(db: &Db, opts: &SyncOpts) -> Result<usize> {
    let projects = db.list_projects(true)?;
    db.with_tx(|db| {
        let mut n = 0;
        for p in projects {
            if p.archived_at.is_some() {
                continue;
            }
            if sync_project(db, &p, opts)? {
                n += 1;
            }
        }
        Ok(n)
    })
}

/// Enrich a single project. Returns true if the DB row was updated.
pub fn sync_project(db: &Db, p: &Project, opts: &SyncOpts) -> Result<bool> {
    if !opts.force {
        if let Some(synced) = p.enrichment_synced_at {
            if Utc::now() - synced < STALE_AFTER {
                return Ok(false);
            }
        }
    }
    let path = p.path_buf();
    let result = enrich(&path, p.deploy_live_lookup && opts.allow_live_lookup);
    let mut update = ProjectUpdate {
        github_url: result.github_url.clone(),
        launch_instructions: result.launch_instructions.clone(),
        deploy_platform: result.deploy_platform.clone(),
        enrichment_synced_at: Some(Utc::now()),
        ..Default::default()
    };
    // Don't overwrite a user-set name.
    if p.name.is_empty() || p.name == "(unknown)" {
        if let Some(n) = result.name.clone() {
            update.name = Some(n);
        }
    }
    // Don't overwrite a user-set deploy_url.
    if p.deploy_url.as_deref().map_or(true, |s| s.trim().is_empty()) {
        if let Some(url) = result.deploy_url.clone() {
            update.deploy_url = Some(url);
        }
    }
    db.update_project_fields(p.id, &update)?;
    Ok(true)
}

pub fn enrich(path: &Path, allow_live_lookup: bool) -> EnrichmentResult {
    let mut out = EnrichmentResult::default();
    if !path.exists() {
        return out;
    }
    out.github_url = git::remote_origin(path);
    out.launch_instructions = launch::infer(path);
    out.name = launch::infer_name(path);
    if let Some(platform) = deploy::detect_platform(path) {
        out.deploy_platform = Some(platform.as_str().to_string());
        if allow_live_lookup {
            out.deploy_url = deploy::live_lookup_url(platform, path);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enrich_empty_dir_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let e = enrich(dir.path(), false);
        assert!(e.github_url.is_none());
        assert!(e.launch_instructions.is_none());
        assert!(e.deploy_platform.is_none());
    }

    #[test]
    fn enrich_detects_vercel_and_package_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("vercel.json"), "{}").unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"demo","scripts":{"dev":"next dev"}}"#,
        )
        .unwrap();
        let e = enrich(dir.path(), false);
        assert_eq!(e.deploy_platform.as_deref(), Some("vercel"));
        assert_eq!(e.name.as_deref(), Some("demo"));
        assert_eq!(e.launch_instructions.as_deref(), Some("npm run dev"));
    }
}
