use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::paths;

pub struct Db {
    conn: Connection,
}

/// Which counter to bump when recording activity on a project.
#[derive(Debug, Clone, Copy)]
pub enum Touch {
    /// Increment `sessions_started` and set `last_active_at`.
    SessionStart,
    /// Increment `prompts_count` and set `last_active_at`.
    Prompt,
    /// Only set `last_active_at`.
    LastActive,
}

pub fn open_db() -> Result<Db> {
    paths::ensure_tracker_dirs()?;
    open_at(&paths::tracker_db()?)
}

pub fn open_at(path: &Path) -> Result<Db> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let conn = Connection::open(path)
        .with_context(|| format!("opening sqlite at {}", path.display()))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    let db = Db { conn };
    db.migrate()?;
    Ok(db)
}

impl Db {
    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS projects (
                id INTEGER PRIMARY KEY,
                path TEXT UNIQUE NOT NULL,
                name TEXT NOT NULL,
                status TEXT,
                status_manual INTEGER DEFAULT 0,
                github_url TEXT,
                deploy_url TEXT,
                deploy_platform TEXT,
                deploy_instructions TEXT,
                launch_instructions TEXT,
                deploy_live_lookup INTEGER DEFAULT 0,
                first_seen_at TEXT NOT NULL,
                last_active_at TEXT,
                sessions_started INTEGER DEFAULT 0,
                prompts_count INTEGER DEFAULT 0,
                notes TEXT,
                enrichment_synced_at TEXT,
                archived_at TEXT
            );

            CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY,
                project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                session_id TEXT,
                event_type TEXT NOT NULL,
                payload_json TEXT,
                at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_events_project_at
                ON events(project_id, at DESC);

            CREATE INDEX IF NOT EXISTS idx_events_session
                ON events(session_id);

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let value: Option<String> = self
            .conn
            .prepare_cached("SELECT value FROM settings WHERE key = ?1")?
            .query_row(params![key], |r| r.get(0))
            .optional()?;
        Ok(value)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn
            .prepare_cached(
                "INSERT INTO settings (key, value) VALUES (?1, ?2) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            )?
            .execute(params![key, value])?;
        Ok(())
    }

    /// Insert a project if its path is not known, otherwise return the
    /// existing row's id.
    pub fn upsert_project_by_path(&self, path: &Path, name_hint: &str) -> Result<i64> {
        let now = Utc::now();
        if let Some(id) = self.find_id_by_path(path)? {
            return Ok(id);
        }
        let path_str = path.to_string_lossy();
        self.conn.execute(
            r#"
            INSERT INTO projects (path, name, first_seen_at)
            VALUES (?1, ?2, ?3)
            "#,
            params![path_str, name_hint, now.to_rfc3339()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Seed a project with discovery stats (name, first-seen, last-active,
    /// sessions, prompts). When the row already exists, counters are taken as
    /// the MAX of current vs. supplied — we never lose live-recorded activity.
    pub fn seed_project(&self, dp: &crate::discovery::DiscoveredProject) -> Result<i64> {
        let id = self.upsert_project_by_path(&dp.path, &dp.name)?;
        let path_str = dp.path.to_string_lossy();
        let first_iso = dp.first_seen_at.to_rfc3339();
        let last_iso = dp.last_active_at.map(|t| t.to_rfc3339());
        self.conn.execute(
            r#"
            UPDATE projects SET
                name = CASE WHEN name = '' OR name = '(unknown)' THEN ?2 ELSE name END,
                first_seen_at = MIN(first_seen_at, ?3),
                last_active_at = CASE
                    WHEN last_active_at IS NULL THEN ?4
                    WHEN ?4 IS NULL THEN last_active_at
                    WHEN ?4 > last_active_at THEN ?4
                    ELSE last_active_at
                END,
                sessions_started = MAX(sessions_started, ?5),
                prompts_count = MAX(prompts_count, ?6)
            WHERE path = ?1
            "#,
            params![
                path_str,
                dp.name,
                first_iso,
                last_iso,
                dp.sessions_started,
                dp.prompts_count
            ],
        )?;
        Ok(id)
    }

    pub fn find_id_by_path(&self, path: &Path) -> Result<Option<i64>> {
        let path_str = path.to_string_lossy();
        let id = self
            .conn
            .query_row(
                "SELECT id FROM projects WHERE path = ?1",
                params![path_str],
                |r| r.get::<_, i64>(0),
            )
            .optional()?;
        Ok(id)
    }

    pub fn record_event(
        &self,
        project_id: i64,
        session_id: Option<&str>,
        event_type: &str,
        payload_json: &str,
        at: DateTime<Utc>,
    ) -> Result<()> {
        self.conn
            .prepare_cached(
                r#"
                INSERT INTO events (project_id, session_id, event_type, payload_json, at)
                VALUES (?1, ?2, ?3, ?4, ?5)
                "#,
            )?
            .execute(params![
                project_id,
                session_id,
                event_type,
                payload_json,
                at.to_rfc3339()
            ])?;
        Ok(())
    }

    pub fn touch(&self, project_id: i64, at: DateTime<Utc>, kind: Touch) -> Result<()> {
        let sql = match kind {
            Touch::SessionStart => {
                "UPDATE projects SET sessions_started = sessions_started + 1, \
                 last_active_at = ?2 WHERE id = ?1"
            }
            Touch::Prompt => {
                "UPDATE projects SET prompts_count = prompts_count + 1, \
                 last_active_at = ?2 WHERE id = ?1"
            }
            Touch::LastActive => "UPDATE projects SET last_active_at = ?2 WHERE id = ?1",
        };
        self.conn
            .prepare_cached(sql)?
            .execute(params![project_id, at.to_rfc3339()])?;
        Ok(())
    }

    /// Run a closure inside a SQLite transaction, committing on Ok and
    /// rolling back on Err.
    pub fn with_tx<T, F>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Self) -> Result<T>,
    {
        self.conn.execute_batch("BEGIN")?;
        match f(self) {
            Ok(v) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(v)
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    pub fn list_projects(&self, include_archived: bool) -> Result<Vec<Project>> {
        let sql = if include_archived {
            SELECT_PROJECT_ALL_SQL.to_string()
        } else {
            format!("{SELECT_PROJECT_ALL_SQL} WHERE archived_at IS NULL")
        };
        let sql = format!("{sql} ORDER BY last_active_at DESC NULLS LAST, name ASC");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], project_from_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn get_project(&self, id: i64) -> Result<Option<Project>> {
        let sql = format!("{SELECT_PROJECT_ALL_SQL} WHERE id = ?1");
        let p = self
            .conn
            .query_row(&sql, params![id], project_from_row)
            .optional()?;
        Ok(p)
    }

    pub fn get_project_by_path(&self, path: &Path) -> Result<Option<Project>> {
        let sql = format!("{SELECT_PROJECT_ALL_SQL} WHERE path = ?1");
        let path_str = path.to_string_lossy();
        let p = self
            .conn
            .query_row(&sql, params![path_str], project_from_row)
            .optional()?;
        Ok(p)
    }

    pub fn update_project_fields(&self, id: i64, fields: &ProjectUpdate) -> Result<()> {
        // Build dynamic UPDATE only for Some() fields so callers can partial-update.
        let mut sets: Vec<&str> = Vec::new();
        let mut vals: Vec<rusqlite::types::Value> = Vec::new();
        macro_rules! push {
            ($field:ident, $col:literal) => {
                if let Some(v) = &fields.$field {
                    sets.push(concat!($col, " = ?"));
                    vals.push(v.clone().into());
                }
            };
        }
        push!(name, "name");
        push!(status, "status");
        push!(github_url, "github_url");
        push!(deploy_url, "deploy_url");
        push!(deploy_platform, "deploy_platform");
        push!(deploy_instructions, "deploy_instructions");
        push!(launch_instructions, "launch_instructions");
        push!(notes, "notes");
        if let Some(v) = fields.status_manual {
            sets.push("status_manual = ?");
            vals.push((v as i64).into());
        }
        if let Some(v) = fields.deploy_live_lookup {
            sets.push("deploy_live_lookup = ?");
            vals.push((v as i64).into());
        }
        if let Some(v) = fields.archived {
            let at: Option<String> = if v {
                Some(Utc::now().to_rfc3339())
            } else {
                None
            };
            sets.push("archived_at = ?");
            match at {
                Some(s) => vals.push(s.into()),
                None => vals.push(rusqlite::types::Value::Null),
            }
        }
        if let Some(v) = &fields.enrichment_synced_at {
            sets.push("enrichment_synced_at = ?");
            vals.push(v.to_rfc3339().into());
        }
        if sets.is_empty() {
            return Ok(());
        }
        let sql = format!(
            "UPDATE projects SET {} WHERE id = ?",
            sets.join(", ")
        );
        vals.push(id.into());
        let params_iter = rusqlite::params_from_iter(vals.iter());
        self.conn.execute(&sql, params_iter)?;
        Ok(())
    }

    pub fn recent_active(&self, limit: usize) -> Result<Vec<Project>> {
        let sql = format!(
            "{SELECT_PROJECT_ALL_SQL} WHERE archived_at IS NULL AND last_active_at IS NOT NULL \
             ORDER BY last_active_at DESC LIMIT ?1"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![limit as i64], project_from_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Project {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub status: Option<String>,
    pub status_manual: bool,
    pub github_url: Option<String>,
    pub deploy_url: Option<String>,
    pub deploy_platform: Option<String>,
    pub deploy_instructions: Option<String>,
    pub launch_instructions: Option<String>,
    pub deploy_live_lookup: bool,
    pub first_seen_at: DateTime<Utc>,
    pub last_active_at: Option<DateTime<Utc>>,
    pub sessions_started: i64,
    pub prompts_count: i64,
    pub notes: Option<String>,
    pub enrichment_synced_at: Option<DateTime<Utc>>,
    pub archived_at: Option<DateTime<Utc>>,
}

impl Project {
    pub fn path_buf(&self) -> PathBuf {
        PathBuf::from(&self.path)
    }
}

#[derive(Debug, Default)]
pub struct ProjectUpdate {
    pub name: Option<String>,
    pub status: Option<String>,
    pub status_manual: Option<bool>,
    pub github_url: Option<String>,
    pub deploy_url: Option<String>,
    pub deploy_platform: Option<String>,
    pub deploy_instructions: Option<String>,
    pub launch_instructions: Option<String>,
    pub deploy_live_lookup: Option<bool>,
    pub notes: Option<String>,
    pub archived: Option<bool>,
    pub enrichment_synced_at: Option<DateTime<Utc>>,
}

const SELECT_PROJECT_ALL_SQL: &str = r#"
    SELECT id, path, name, status, status_manual, github_url, deploy_url, deploy_platform,
           deploy_instructions, launch_instructions, deploy_live_lookup, first_seen_at,
           last_active_at, sessions_started, prompts_count, notes, enrichment_synced_at,
           archived_at
    FROM projects
"#;

fn project_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Project> {
    fn parse_dt(s: Option<String>) -> Option<DateTime<Utc>> {
        s.and_then(|v| DateTime::parse_from_rfc3339(&v).ok())
            .map(|d| d.with_timezone(&Utc))
    }
    Ok(Project {
        id: r.get(0)?,
        path: r.get(1)?,
        name: r.get(2)?,
        status: r.get(3)?,
        status_manual: r.get::<_, i64>(4)? != 0,
        github_url: r.get(5)?,
        deploy_url: r.get(6)?,
        deploy_platform: r.get(7)?,
        deploy_instructions: r.get(8)?,
        launch_instructions: r.get(9)?,
        deploy_live_lookup: r.get::<_, i64>(10)? != 0,
        first_seen_at: parse_dt(r.get(11)?).unwrap_or_else(Utc::now),
        last_active_at: parse_dt(r.get(12)?),
        sessions_started: r.get(13)?,
        prompts_count: r.get(14)?,
        notes: r.get(15)?,
        enrichment_synced_at: parse_dt(r.get(16)?),
        archived_at: parse_dt(r.get(17)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_db() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db = open_at(&dir.path().join("t.sqlite")).unwrap();
        (dir, db)
    }

    #[test]
    fn upsert_is_idempotent() {
        let (_d, db) = tmp_db();
        let p = Path::new("/tmp/foo");
        let a = db.upsert_project_by_path(p, "foo").unwrap();
        let b = db.upsert_project_by_path(p, "foo").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn touch_increments() {
        let (_d, db) = tmp_db();
        let id = db
            .upsert_project_by_path(Path::new("/tmp/proj"), "proj")
            .unwrap();
        db.touch(id, Utc::now(), Touch::SessionStart).unwrap();
        db.touch(id, Utc::now(), Touch::Prompt).unwrap();
        db.touch(id, Utc::now(), Touch::Prompt).unwrap();
        let p = db.get_project(id).unwrap().unwrap();
        assert_eq!(p.sessions_started, 1);
        assert_eq!(p.prompts_count, 2);
        assert!(p.last_active_at.is_some());
    }

    #[test]
    fn update_fields_partial() {
        let (_d, db) = tmp_db();
        let id = db
            .upsert_project_by_path(Path::new("/tmp/proj"), "proj")
            .unwrap();
        db.update_project_fields(
            id,
            &ProjectUpdate {
                deploy_url: Some("https://x.example".into()),
                deploy_platform: Some("vercel".into()),
                status_manual: Some(true),
                status: Some("developing".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let p = db.get_project(id).unwrap().unwrap();
        assert_eq!(p.deploy_url.as_deref(), Some("https://x.example"));
        assert_eq!(p.deploy_platform.as_deref(), Some("vercel"));
        assert!(p.status_manual);
        assert_eq!(p.status.as_deref(), Some("developing"));
    }

    #[test]
    fn settings_upsert() {
        let (_d, db) = tmp_db();
        assert_eq!(db.get_setting("pref").unwrap(), None);
        db.set_setting("pref", "ghostty").unwrap();
        assert_eq!(db.get_setting("pref").unwrap().as_deref(), Some("ghostty"));
        db.set_setting("pref", "wezterm").unwrap();
        assert_eq!(db.get_setting("pref").unwrap().as_deref(), Some("wezterm"));
    }

    #[test]
    fn list_and_archive() {
        let (_d, db) = tmp_db();
        let a = db.upsert_project_by_path(Path::new("/tmp/a"), "a").unwrap();
        let _b = db.upsert_project_by_path(Path::new("/tmp/b"), "b").unwrap();
        db.update_project_fields(
            a,
            &ProjectUpdate {
                archived: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(db.list_projects(false).unwrap().len(), 1);
        assert_eq!(db.list_projects(true).unwrap().len(), 2);
    }
}
