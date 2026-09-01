//! Bounded, work-conserving scheduling for one renderer-owned Fetch batch.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) fn execute_bounded<T, F>(items: Vec<T>, parallelism: usize, execute: F) -> u64
where
    T: Send,
    F: Fn(T) -> u64 + Sync,
{
    if items.is_empty() || parallelism == 0 {
        return 0;
    }
    let worker_count = items.len().min(parallelism);
    let queue = Mutex::new(VecDeque::from(items));
    let total = AtomicU64::new(0);
    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            scope.spawn(|| {
                loop {
                    let item = queue
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .pop_front();
                    let Some(item) = item else {
                        break;
                    };
                    total.fetch_add(execute(item), Ordering::Relaxed);
                }
            });
        }
    });
    total.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Condvar};
    use std::time::Duration;

    #[test]
    fn starts_queued_work_before_a_slow_peer_finishes() {
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let release_slow = Arc::clone(&release);
        std::thread::scope(|scope| {
            let worker = scope.spawn(move || {
                execute_bounded(vec![0_u8, 1, 2], 2, |item| {
                    started_tx.send(item).unwrap();
                    if item == 0 {
                        let (lock, changed) = &*release_slow;
                        let mut ready = lock.lock().unwrap();
                        while !*ready {
                            ready = changed.wait(ready).unwrap();
                        }
                    }
                    u64::from(item) + 1
                })
            });

            let mut first = [
                started_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
                started_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            ];
            first.sort_unstable();
            assert_eq!(first, [0, 1]);
            assert_eq!(
                started_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
                2,
                "the free slot should start the next request without waiting for its slow peer"
            );
            let (lock, changed) = &*release;
            *lock.lock().unwrap() = true;
            changed.notify_one();
            assert_eq!(worker.join().unwrap(), 6);
        });
    }
}
