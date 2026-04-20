pub mod db;
pub mod discovery;
pub mod encode;
pub mod hooks;
pub mod ingest;
pub mod os;
pub mod paths;
pub mod plugin;
pub mod status;
pub mod sync;
pub mod terminal;

pub use db::open_db;

use chrono::{DateTime, Utc};

pub fn parse_rfc3339(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}
