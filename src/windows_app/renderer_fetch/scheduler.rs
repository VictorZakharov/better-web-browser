//! Bounded, work-conserving scheduling for one renderer-owned Fetch batch.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

pub(super) enum Step<T> {
    Ready(T),
    Parked(T),
    Done(u64),
}

pub(super) fn execute_bounded<T, F>(items: Vec<T>, parallelism: usize, execute: F) -> u64
where
    T: Send,
    F: Fn(T) -> Step<T> + Sync,
{
    if items.is_empty() || parallelism == 0 {
        return 0;
    }
    let worker_count = items.len().min(parallelism);
    let remaining = items.len();
    let queue = Mutex::new((
        items
            .into_iter()
            .map(|item| (Instant::now(), item))
            .collect::<VecDeque<_>>(),
        remaining,
    ));
    let changed = Condvar::new();
    let total = AtomicU64::new(0);
    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            scope.spawn(|| {
                loop {
                    let item = {
                        let mut state = queue.lock().unwrap_or_else(|p| p.into_inner());
                        loop {
                            if state.1 == 0 {
                                return;
                            }
                            if let Some(index) =
                                state.0.iter().position(|(due, _)| *due <= Instant::now())
                            {
                                break state.0.remove(index).unwrap().1;
                            }
                            let wait = state
                                .0
                                .iter()
                                .map(|(due, _)| due.saturating_duration_since(Instant::now()))
                                .min()
                                .unwrap_or(Duration::from_millis(5));
                            state = changed
                                .wait_timeout(state, wait)
                                .unwrap_or_else(|p| p.into_inner())
                                .0;
                        }
                    };
                    let step = execute(item);
                    let mut state = queue.lock().unwrap_or_else(|p| p.into_inner());
                    match step {
                        Step::Ready(item) => state.0.push_back((Instant::now(), item)),
                        // Backpressure parks a response, not a network worker. A bounded poll also
                        // notices document retirement without adding one thread per response.
                        Step::Parked(item) => state
                            .0
                            .push_back((Instant::now() + Duration::from_millis(5), item)),
                        Step::Done(bytes) => {
                            state.1 -= 1;
                            total.fetch_add(bytes, Ordering::Relaxed);
                        }
                    }
                    changed.notify_all();
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

    fn observe_starts_and_release(
        receiver: &std::sync::mpsc::Receiver<u8>,
        release: &(Mutex<bool>, Condvar),
        timeout: Duration,
    ) -> Result<Vec<u8>, std::sync::mpsc::RecvTimeoutError> {
        let started = (0..3).map(|_| receiver.recv_timeout(timeout)).collect();
        // Release before any assertion or propagated receive error. Otherwise
        // scope unwinding joins a worker that is waiting for the failed test.
        *release.0.lock().unwrap() = true;
        release.1.notify_all();
        started
    }

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
                    Step::Done(u64::from(item) + 1)
                })
            });

            // Queue ownership is ordered, but sends from different workers are
            // not. Observe all starts while the slow peer is held, in any order.
            let started = observe_starts_and_release(&started_rx, &release, Duration::from_secs(2));
            let bytes = worker.join().unwrap();
            let mut started =
                started.expect("all requests must start before releasing the slow peer");
            started.sort_unstable();
            assert_eq!(
                started,
                [0, 1, 2],
                "the free slot should start the next request without waiting for its slow peer"
            );
            assert_eq!(bytes, 6);
        });
    }

    #[test]
    fn start_observation_accepts_cross_worker_message_order() {
        let (sender, receiver) = std::sync::mpsc::channel();
        for item in [1, 2, 0] {
            sender.send(item).unwrap();
        }
        let release = (Mutex::new(false), Condvar::new());
        assert_eq!(
            observe_starts_and_release(&receiver, &release, Duration::ZERO).unwrap(),
            [1, 2, 0]
        );
        assert!(*release.0.lock().unwrap());
    }

    #[test]
    fn failed_start_observation_still_releases_slow_peer() {
        let (_sender, receiver) = std::sync::mpsc::channel();
        let release = (Mutex::new(false), Condvar::new());
        assert!(observe_starts_and_release(&receiver, &release, Duration::ZERO).is_err());
        assert!(*release.0.lock().unwrap());
    }

    #[test]
    fn parked_bodies_do_not_occupy_slots_needed_by_later_requests() {
        let ready = std::sync::atomic::AtomicBool::new(false);
        let bytes = execute_bounded((0..17).collect(), 8, |id| {
            if id == 16 {
                ready.store(true, Ordering::Release);
            }
            if ready.load(Ordering::Acquire) {
                Step::Done(1)
            } else {
                Step::Parked(id)
            }
        });
        assert_eq!(bytes, 17);
    }
}
