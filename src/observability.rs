use std::{
    collections::HashMap,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use sha2::{Digest, Sha256};

#[derive(Default)]
pub struct Metrics {
    pub uploads_completed: AtomicU64,
    pub uploads_failed: AtomicU64,
    pub downloads_started: AtomicU64,
    pub requests_rejected: AtomicU64,
    pub cleanup_failures: AtomicU64,
    pub shelves_cleaned: AtomicU64,
    pub conversion_millis: AtomicU64,
}

impl Metrics {
    pub fn increment(counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add(counter: &AtomicU64, value: u64) {
        counter.fetch_add(value, Ordering::Relaxed);
    }

    pub fn render(&self, active_shelves: i64, stored_bytes: i64, cleanup_lag: i64) -> String {
        format!(
            "kobo_active_shelves {active_shelves}\nkobo_stored_bytes {stored_bytes}\nkobo_cleanup_lag_seconds {cleanup_lag}\nkobo_uploads_completed {}\nkobo_uploads_failed {}\nkobo_downloads_started {}\nkobo_requests_rejected {}\nkobo_cleanup_failures {}\nkobo_shelves_cleaned {}\nkobo_conversion_milliseconds_total {}\n",
            self.uploads_completed.load(Ordering::Relaxed),
            self.uploads_failed.load(Ordering::Relaxed),
            self.downloads_started.load(Ordering::Relaxed),
            self.requests_rejected.load(Ordering::Relaxed),
            self.cleanup_failures.load(Ordering::Relaxed),
            self.shelves_cleaned.load(Ordering::Relaxed),
            self.conversion_millis.load(Ordering::Relaxed),
        )
    }
}

struct Window {
    started: Instant,
    count: u32,
}

#[derive(Default)]
pub struct RateLimiter {
    windows: Mutex<HashMap<String, Window>>,
}

impl RateLimiter {
    pub fn allow(&self, category: &str, secret: &str, limit: u32, duration: Duration) -> bool {
        let digest = Sha256::digest(secret.as_bytes());
        let key = format!("{category}:{}", hex_prefix(&digest));
        let now = Instant::now();
        let mut windows = self.windows.lock().unwrap();
        windows.retain(|_, window| now.duration_since(window.started) < Duration::from_secs(3600));
        let window = windows.entry(key).or_insert(Window {
            started: now,
            count: 0,
        });
        if now.duration_since(window.started) >= duration {
            window.started = now;
            window.count = 0;
        }
        if window.count >= limit {
            return false;
        }
        window.count += 1;
        true
    }
}

fn hex_prefix(bytes: &[u8]) -> String {
    bytes[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_is_bounded_and_separates_secret_keys() {
        let limiter = RateLimiter::default();
        assert!(limiter.allow("upload", "one", 1, Duration::from_secs(60)));
        assert!(!limiter.allow("upload", "one", 1, Duration::from_secs(60)));
        assert!(limiter.allow("upload", "two", 1, Duration::from_secs(60)));
    }
}
