use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Planning,
    Developing,
    Deployed,
    Active,
    Idle,
    Stale,
    Archived,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Planning => "planning",
            Status::Developing => "developing",
            Status::Deployed => "deployed",
            Status::Active => "active",
            Status::Idle => "idle",
            Status::Stale => "stale",
            Status::Archived => "archived",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "planning" => Some(Self::Planning),
            "developing" => Some(Self::Developing),
            "deployed" => Some(Self::Deployed),
            "active" => Some(Self::Active),
            "idle" => Some(Self::Idle),
            "stale" => Some(Self::Stale),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }
}

pub struct StatusInputs<'a> {
    pub last_active_at: Option<DateTime<Utc>>,
    pub deploy_url: Option<&'a str>,
    pub archived_at: Option<DateTime<Utc>>,
    pub now: DateTime<Utc>,
}

/// Infer a status from activity timing and deploy state. Called only when a
/// project row does not have a user-set `status_manual` value.
pub fn infer(inputs: &StatusInputs<'_>) -> Status {
    if inputs.archived_at.is_some() {
        return Status::Archived;
    }
    if inputs.deploy_url.map_or(false, |u| !u.trim().is_empty()) {
        return Status::Deployed;
    }
    let Some(last) = inputs.last_active_at else {
        return Status::Stale;
    };
    let age = inputs.now - last;
    if age < Duration::days(7) {
        Status::Active
    } else if age < Duration::days(30) {
        Status::Idle
    } else {
        Status::Stale
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 4, day, 12, 0, 0).unwrap()
    }

    #[test]
    fn deploy_url_beats_activity() {
        let s = infer(&StatusInputs {
            last_active_at: Some(at(1)),
            deploy_url: Some("https://x.example"),
            archived_at: None,
            now: at(17),
        });
        assert_eq!(s, Status::Deployed);
    }

    #[test]
    fn archive_beats_everything() {
        let s = infer(&StatusInputs {
            last_active_at: Some(at(17)),
            deploy_url: Some("https://x.example"),
            archived_at: Some(at(15)),
            now: at(17),
        });
        assert_eq!(s, Status::Archived);
    }

    #[test]
    fn active_within_seven_days() {
        let s = infer(&StatusInputs {
            last_active_at: Some(at(12)),
            deploy_url: None,
            archived_at: None,
            now: at(17),
        });
        assert_eq!(s, Status::Active);
    }

    #[test]
    fn idle_between_seven_and_thirty() {
        let s = infer(&StatusInputs {
            last_active_at: Some(at(1)),
            deploy_url: None,
            archived_at: None,
            now: at(17),
        });
        assert_eq!(s, Status::Idle);
    }

    #[test]
    fn stale_after_thirty() {
        let s = infer(&StatusInputs {
            last_active_at: Some(at(1)),
            deploy_url: None,
            archived_at: None,
            now: Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).unwrap(),
        });
        assert_eq!(s, Status::Stale);
    }
}
