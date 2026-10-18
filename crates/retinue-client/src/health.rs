use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
struct Evidence {
    state: &'static str,
    checked_at: u64,
    error: Option<String>,
    last_failure: Option<Failure>,
}

#[derive(Clone, Debug, Serialize)]
struct Failure {
    checked_at: u64,
    error: String,
}

#[derive(Default, Debug)]
pub struct Health {
    checks: Mutex<Checks>,
    revision: AtomicU64,
}

#[derive(Default, Debug)]
struct Checks {
    entries: BTreeMap<&'static str, Evidence>,
}

impl Health {
    pub fn record<T>(&self, check: &'static str, result: &Result<T, String>) {
        let error = result
            .as_ref()
            .err()
            .map(|error| error.chars().take(2048).collect::<String>());
        let mut checks = self.checks.lock().unwrap();
        if check.is_empty()
            || check.len() > 128
            || (checks.entries.len() >= 64 && !checks.entries.contains_key(check))
        {
            return;
        }
        let previous = checks.entries.get(check);
        if previous.is_none_or(|old| old.error != error) {
            self.revision.fetch_add(1, Ordering::Release);
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let checked_at = previous.map_or(now, |old| now.max(old.checked_at + 1));
        // Preserve a failure even if recovery is coalesced into the same outbound snapshot.
        let last_failure = match &error {
            Some(error) => Some(Failure {
                checked_at,
                error: error.clone(),
            }),
            None => previous.and_then(|old| old.last_failure.clone()),
        };
        checks.entries.insert(
            check,
            Evidence {
                state: if error.is_some() { "fail" } else { "pass" },
                checked_at,
                error,
                last_failure,
            },
        );
    }

    pub fn snapshot(&self) -> (u64, serde_json::Value) {
        let checks = self.checks.lock().unwrap();
        (
            self.revision(),
            serde_json::to_value(&checks.entries).unwrap(),
        )
    }

    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_requires_a_run_and_recovery_requires_success() {
        let health = Health::default();
        assert_eq!(health.snapshot().1, serde_json::json!({}));
        health.record::<()>("native_inventory", &Err("layout changed".into()));
        health.record::<()>("native_inventory", &Err("layout changed".into()));
        assert_eq!(health.snapshot().0, 1);
        assert_eq!(health.snapshot().1["native_inventory"]["state"], "fail");
        health.record("native_inventory", &Ok(()));
        assert_eq!(health.snapshot().0, 2);
        assert_eq!(health.snapshot().1["native_inventory"]["state"], "pass");
        assert!(health.snapshot().1["native_inventory"]["error"].is_null());
        assert_eq!(
            health.snapshot().1["native_inventory"]["last_failure"]["error"],
            "layout changed"
        );
        for _ in 0..1000 {
            health.record("native_inventory", &Ok(()));
        }
        assert_eq!(health.revision(), 2);
        assert_eq!(Health::default().snapshot().1, serde_json::json!({}));
    }

    #[test]
    fn check_names_have_a_fixed_bound() {
        let health = Health::default();
        health.record("", &Ok(()));
        assert_eq!(health.revision(), 0);
        health.record("native_state", &Ok(()));
        assert!(health.snapshot().1["native_state"]["last_failure"].is_null());
    }
}
