use std::sync::atomic::{AtomicU64, Ordering};

/// Simple atomic counters for observability. These can be exported
/// via Prometheus, logged, or queried via the status endpoint.
#[derive(Debug, Default)]
pub struct Metrics {
    pub events_published: AtomicU64,
    pub events_routed: AtomicU64,
    pub jobs_created: AtomicU64,
    pub jobs_succeeded: AtomicU64,
    pub jobs_failed: AtomicU64,
    pub gates_passed: AtomicU64,
    pub gates_failed: AtomicU64,
    pub iterations: AtomicU64,
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn inc_events_published(&self) {
        self.events_published.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_events_routed(&self) {
        self.events_routed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_jobs_created(&self) {
        self.jobs_created.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_jobs_succeeded(&self) {
        self.jobs_succeeded.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_jobs_failed(&self) {
        self.jobs_failed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_gates_passed(&self) {
        self.gates_passed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_gates_failed(&self) {
        self.gates_failed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_iterations(&self) {
        self.iterations.fetch_add(1, Ordering::Relaxed);
    }

    /// Snapshot all metrics as a JSON-friendly struct.
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            events_published: self.events_published.load(Ordering::Relaxed),
            events_routed: self.events_routed.load(Ordering::Relaxed),
            jobs_created: self.jobs_created.load(Ordering::Relaxed),
            jobs_succeeded: self.jobs_succeeded.load(Ordering::Relaxed),
            jobs_failed: self.jobs_failed.load(Ordering::Relaxed),
            gates_passed: self.gates_passed.load(Ordering::Relaxed),
            gates_failed: self.gates_failed.load(Ordering::Relaxed),
            iterations: self.iterations.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, serde::Serialize)]
pub struct MetricsSnapshot {
    pub events_published: u64,
    pub events_routed: u64,
    pub jobs_created: u64,
    pub jobs_succeeded: u64,
    pub jobs_failed: u64,
    pub gates_passed: u64,
    pub gates_failed: u64,
    pub iterations: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_increment() {
        let m = Metrics::new();
        m.inc_jobs_created();
        m.inc_jobs_created();
        m.inc_jobs_succeeded();

        let snap = m.snapshot();
        assert_eq!(snap.jobs_created, 2);
        assert_eq!(snap.jobs_succeeded, 1);
        assert_eq!(snap.jobs_failed, 0);
    }

    #[test]
    fn test_metrics_snapshot_serializes() {
        let m = Metrics::new();
        m.inc_events_published();
        let snap = m.snapshot();
        let json = serde_json::to_string(&snap).unwrap();
        assert!(json.contains("\"events_published\":1"));
    }
}
