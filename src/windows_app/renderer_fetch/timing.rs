//! When each request of a renderer Fetch batch was admitted, started, answered and finished,
//! for the hidden benchmark's request timeline.

use super::*;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

pub(in crate::windows_app) struct FetchTiming {
    pub(in crate::windows_app) url: String,
    pub(in crate::windows_app) destination: ResourceDestination,
    pub(in crate::windows_app) initiator: FetchInitiator,
    /// The batch reached the browser's network scheduler.
    pub(in crate::windows_app) admitted: Instant,
    /// A network worker began the request.
    pub(in crate::windows_app) started: Option<Instant>,
    /// The network delivered the response head.
    pub(in crate::windows_app) received: Option<Instant>,
    /// The response head was handed to the renderer.
    pub(in crate::windows_app) headers: Option<Instant>,
    /// The body ended, or the request failed.
    pub(in crate::windows_app) finished: Option<Instant>,
}

#[derive(Default)]
pub(super) struct BatchTimings(Mutex<HashMap<u64, FetchTiming>>);

impl BatchTimings {
    pub(super) fn admit(&self, request: &RendererFetchRequest, admitted: Instant) {
        self.lock()
            .entry(request.head.request_id)
            .or_insert(FetchTiming {
                url: request.head.url.clone(),
                destination: request.head.destination,
                initiator: request.head.initiator,
                admitted,
                started: None,
                received: None,
                headers: None,
                finished: None,
            });
    }

    /// Records one scheduling step of a request: its first step starts it, the step that leaves
    /// a body to stream delivered its head, and the final step finishes it.
    pub(super) fn step(
        &self,
        id: u64,
        first: bool,
        before: Instant,
        received: Option<Instant>,
        done: bool,
    ) {
        let now = Instant::now();
        self.update(id, |timing| {
            if first {
                timing.started = Some(before);
                if !done {
                    timing.received = received;
                    timing.headers = Some(now);
                }
            }
            if done {
                timing.finished = Some(now);
            }
        });
    }

    pub(super) fn into_sorted(self) -> Vec<FetchTiming> {
        let mut timings = self
            .0
            .into_inner()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .into_values()
            .collect::<Vec<_>>();
        timings.sort_by_key(|timing| (timing.started, timing.admitted));
        timings
    }

    fn update(&self, id: u64, apply: impl FnOnce(&mut FetchTiming)) {
        if let Some(timing) = self.lock().get_mut(&id) {
            apply(timing);
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u64, FetchTiming>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
