//! Parse Claude Code session transcripts (`.jsonl` files under
//! `~/.claude/projects/<encoded>/`) to extract the project cwd and high-level
//! activity counters.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Deserialize;

/// Summary of a single `.jsonl` transcript file.
#[derive(Debug, Default)]
pub struct TranscriptSummary {
    pub session_id: Option<String>,
    pub cwd: Option<PathBuf>,
    pub first_at: Option<DateTime<Utc>>,
    pub last_at: Option<DateTime<Utc>>,
    pub user_prompts: i64,
}

#[derive(Deserialize)]
struct Row {
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default, rename = "sessionId")]
    session_id: Option<String>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    message: Option<MsgRef>,
    #[serde(default, rename = "isSidechain")]
    is_sidechain: Option<bool>,
}

#[derive(Deserialize)]
struct MsgRef {
    #[serde(default)]
    role: Option<String>,
}

pub fn summarize_file(path: &Path) -> Result<TranscriptSummary> {
    let f = File::open(path)?;
    let reader = BufReader::new(f);
    let mut summary = TranscriptSummary::default();
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(row) = serde_json::from_str::<Row>(&line) else {
            continue;
        };
        if summary.cwd.is_none() {
            if let Some(c) = row.cwd.as_ref() {
                summary.cwd = Some(PathBuf::from(c));
            }
        }
        if summary.session_id.is_none() {
            summary.session_id = row.session_id.clone();
        }
        if let Some(ts) = row.timestamp.as_ref().and_then(parse_dt) {
            if summary.first_at.map_or(true, |f| ts < f) {
                summary.first_at = Some(ts);
            }
            if summary.last_at.map_or(true, |l| ts > l) {
                summary.last_at = Some(ts);
            }
        }
        if row.r#type.as_deref() == Some("user")
            && row.is_sidechain != Some(true)
            && row.message.as_ref().and_then(|m| m.role.as_deref()) == Some("user")
        {
            summary.user_prompts += 1;
        }
    }
    Ok(summary)
}

fn parse_dt(s: &String) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}
