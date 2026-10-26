//! `{app_data}/updates/pending.json`: written right before an install hands
//! off, read on the next launch to tell success from a failed install.

use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

const EXPIRE_AFTER_DAYS: i64 = 7;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    pub from: String,
    pub to: String,
    pub started_at: DateTime<Utc>,
    pub attempts: u32,
    /// The failure was already surfaced once; do not repeat the notice.
    #[serde(default)]
    pub reported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootOutcome {
    Nothing,
    Succeeded { to: String },
    /// Install did not take; `notify` is false after the first report.
    Failed {
        version: String,
        attempts: u32,
        notify: bool,
    },
    Expired,
}

pub fn pending_path() -> PathBuf {
    super::download::updates_root().join("pending.json")
}

pub fn read() -> Option<Pending> {
    let raw = fs::read_to_string(pending_path()).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn write(p: &Pending) -> Result<(), String> {
    let path = pending_path();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("create updates dir: {e}"))?;
    }
    let tmp = path.with_extension("json.tmp");
    let body = serde_json::to_vec_pretty(p).map_err(|e| e.to_string())?;
    fs::write(&tmp, body).map_err(|e| format!("write pending: {e}"))?;
    fs::rename(&tmp, &path).map_err(|e| format!("commit pending: {e}"))
}

pub fn clear() {
    let _ = fs::remove_file(pending_path());
}

/// Next marker before handing off an install of `to` from `current`.
pub fn next_attempt(prev: Option<&Pending>, current: &str, to: &str, now: DateTime<Utc>) -> Pending {
    let attempts = match prev {
        Some(p) if p.to == to => p.attempts.saturating_add(1),
        _ => 1,
    };
    Pending {
        from: current.to_string(),
        to: to.to_string(),
        started_at: now,
        attempts,
        reported: false,
    }
}

/// Decide what the previous install did. Returns the marker to keep (if any).
pub fn evaluate(
    pending: Option<Pending>,
    current: &str,
    now: DateTime<Utc>,
) -> (BootOutcome, Option<Pending>) {
    let Some(p) = pending else {
        return (BootOutcome::Nothing, None);
    };
    if p.to == current {
        return (BootOutcome::Succeeded { to: p.to }, None);
    }
    if now - p.started_at > Duration::days(EXPIRE_AFTER_DAYS) {
        return (BootOutcome::Expired, None);
    }
    let notify = !p.reported;
    let outcome = BootOutcome::Failed {
        version: p.to.clone(),
        attempts: p.attempts,
        notify,
    };
    (outcome, Some(Pending { reported: true, ..p }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(h: i64) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(1_700_000_000 + h * 3600, 0).unwrap()
    }

    #[test]
    fn attempts_count_per_target_version() {
        let first = next_attempt(None, "1.0.0", "1.1.0", at(0));
        assert_eq!(first.attempts, 1);
        let second = next_attempt(Some(&first), "1.0.0", "1.1.0", at(1));
        assert_eq!(second.attempts, 2);
        let other = next_attempt(Some(&second), "1.0.0", "1.2.0", at(2));
        assert_eq!(other.attempts, 1);
    }

    #[test]
    fn evaluate_success_failure_and_expiry() {
        let p = next_attempt(None, "1.0.0", "1.1.0", at(0));
        assert_eq!(evaluate(None, "1.0.0", at(1)), (BootOutcome::Nothing, None));

        let (o, keep) = evaluate(Some(p.clone()), "1.1.0", at(1));
        assert_eq!(o, BootOutcome::Succeeded { to: "1.1.0".into() });
        assert!(keep.is_none());

        let (o, keep) = evaluate(Some(p.clone()), "1.0.0", at(1));
        assert_eq!(
            o,
            BootOutcome::Failed { version: "1.1.0".into(), attempts: 1, notify: true }
        );
        let keep = keep.unwrap();
        assert!(keep.reported);

        let (o, _) = evaluate(Some(keep), "1.0.0", at(2));
        assert_eq!(
            o,
            BootOutcome::Failed { version: "1.1.0".into(), attempts: 1, notify: false }
        );

        let (o, keep) = evaluate(Some(p), "1.0.0", at(24 * 8));
        assert_eq!(o, BootOutcome::Expired);
        assert!(keep.is_none());
    }
}
