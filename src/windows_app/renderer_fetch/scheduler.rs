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
