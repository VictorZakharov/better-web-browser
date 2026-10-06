//! Browser-owned concurrent hardware admission, shared by Window and Worker.
//! HTML permits a reduced count when the user agent limits concurrent workers.
//! Do not expose more processors than our dedicated-worker admission supports.
//! https://html.spec.whatwg.org/multipage/workers.html#navigator.hardwareconcurrency

use std::sync::OnceLock;

pub(crate) const MAX_DEDICATED_WORKERS: usize = 16;

pub(in crate::engine::script) fn hardware_concurrency() -> usize {
    static COUNT: OnceLock<usize> = OnceLock::new();
    *COUNT.get_or_init(|| {
        admitted_processors(std::thread::available_parallelism().ok().map(usize::from))
    })
}

fn admitted_processors(available: Option<usize>) -> usize {
    available.unwrap_or(1).clamp(1, MAX_DEDICATED_WORKERS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_or_invalid_capacity_fails_to_one_not_zero() {
        assert_eq!(admitted_processors(None), 1);
        assert_eq!(admitted_processors(Some(0)), 1);
    }

    #[test]
    fn advertised_capacity_does_not_exceed_the_worker_admission_limit() {
        for available in 1..=256 {
            let count = admitted_processors(Some(available));
            assert!(count >= 1 && count <= available && count <= MAX_DEDICATED_WORKERS);
            assert_eq!(count, available.min(MAX_DEDICATED_WORKERS));
        }
        assert_eq!(admitted_processors(Some(usize::MAX)), MAX_DEDICATED_WORKERS);
    }

    #[test]
    fn processor_count_is_stable_across_execution_threads() {
        let count = hardware_concurrency();
        assert_eq!(
            std::thread::spawn(hardware_concurrency).join().unwrap(),
            count
        );
    }
}
