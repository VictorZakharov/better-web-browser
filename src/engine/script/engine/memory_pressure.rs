//! Process-wide pressure signals consumed once by each V8 agent at task boundaries.
//! Heaps share the renderer Job, but retain V8's own allocation/collection policy.
//! Notifications do not enlarge that Job, change a script deadline, or discard live data.
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

mod policy;
pub(super) use policy::{Advice, Level};
mod profile;
pub(super) use profile::Profile;

const SAMPLE_INTERVAL: Duration = Duration::from_millis(250);

struct Monitor {
    policy: policy::Policy,
    sampled: Option<Instant>,
    available: bool,
}

impl Default for Monitor {
    fn default() -> Self {
        Self {
            policy: policy::Policy::new(crate::renderer_budget::current().bytes()),
            sampled: None,
            available: false,
        }
    }
}

pub(super) fn advice() -> Option<Advice> {
    static MONITOR: OnceLock<Mutex<Monitor>> = OnceLock::new();
    let mut monitor = MONITOR.get_or_init(Default::default).lock().ok()?;
    let now = Instant::now();
    if monitor
        .sampled
        .is_none_or(|sampled| now.saturating_duration_since(sampled) >= SAMPLE_INTERVAL)
    {
        monitor.sampled = Some(now);
        let bytes = crate::process_memory::current().map(|sample| sample.private);
        monitor.available = bytes.is_some();
        monitor.policy.observe(now, bytes);
    }
    // An unavailable sample is neither zero use nor evidence of pressure recovery.
    monitor.available.then(|| monitor.policy.advice())
}

#[derive(Default)]
pub(super) struct Consumer {
    last_epoch: u64,
}

impl Consumer {
    pub(super) fn pending(&self, advice: Advice) -> bool {
        advice.epoch != self.last_epoch
    }
    pub(super) fn delivered(&mut self, advice: Advice) {
        self.last_epoch = advice.epoch;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn related_realms_share_an_agent_but_each_worker_can_consume_the_signal() {
        let mut document = Consumer::default();
        let mut worker = Consumer::default();
        let mut policy = policy::Policy::new(1024);
        let now = Instant::now();
        policy.observe(now, Some(800));
        let advice = policy.advice();
        assert!(document.pending(advice) && worker.pending(advice));
        document.delivered(advice);
        assert!(!document.pending(advice));
        assert!(worker.pending(advice));
        worker.delivered(advice);
        assert!(!worker.pending(advice));
        policy.observe(now + Duration::from_secs(1), Some(300));
        assert!(document.pending(policy.advice()) && worker.pending(policy.advice()));
        assert_eq!(policy.advice().level, Level::None);
    }

    #[test]
    fn no_initial_notification_or_acknowledgement_before_success() {
        let consumer = Consumer::default();
        let policy = policy::Policy::new(1024);
        assert!(!consumer.pending(policy.advice()));
        // Querying without delivering remains pending: cancellation must not acknowledge GC.
        let mut policy = policy;
        policy.observe(Instant::now(), Some(900));
        assert!(consumer.pending(policy.advice()));
        assert!(consumer.pending(policy.advice()));
    }
}
