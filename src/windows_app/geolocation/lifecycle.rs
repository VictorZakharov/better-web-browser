//! UI-thread event delivery, visibility pause, and acquisition deadlines.

use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

impl GeolocationService {
    fn process_event(
        &mut self,
        event: NativeEvent,
        router: &TabMessageRouter,
        visible_tab: Option<TabId>,
    ) {
        match event {
            NativeEvent::Access(key, result) => {
                let Some(active) = self.active.get_mut(&key) else {
                    return;
                };
                if let Err(failure) = result {
                    active.context.emit(error(failure), true);
                    self.active.remove(&key);
                } else {
                    active.access_granted = true;
                    if visible_tab == Some(key.tab) {
                        self.start_source(key, router);
                    }
                }
            }
            NativeEvent::Position(key, generation, result) => {
                let Some(active) = self.active.get_mut(&key) else {
                    return;
                };
                if active.generation != generation || visible_tab != Some(key.tab) {
                    return;
                }
                match result {
                    Ok(position) => {
                        if !position_is_current(
                            &position,
                            active.acquisition_started_millis,
                            active.maximum_age_millis,
                        ) {
                            // WinRT may return a fix older than the requested maximumAge.
                            // A one-shot has no further native callback to await, while a
                            // watch can still receive a fresh PositionChanged event.
                            if !active.watch {
                                active
                                    .context
                                    .emit(error(GeoFailure::PositionUnavailable), true);
                                self.active.remove(&key);
                            }
                            return;
                        }
                        if active.last_timestamp_millis == Some(position.timestamp_millis) {
                            return;
                        }
                        active.last_timestamp_millis = Some(position.timestamp_millis);
                        self.cache.insert(
                            (active.context.origin.clone(), active.high_accuracy),
                            position.clone(),
                        );
                        active
                            .context
                            .emit(GeolocationEvent::Position(position), !active.watch);
                        active.remaining = finite_timeout(active.timeout_millis);
                        active.deadline = active
                            .remaining
                            .filter(|duration| !duration.is_zero())
                            .and_then(|duration| Instant::now().checked_add(duration));
                        if !active.watch {
                            self.active.remove(&key);
                        }
                    }
                    Err(failure) => {
                        active.context.emit(
                            error(failure),
                            !active.watch || failure == GeoFailure::PermissionDenied,
                        );
                        active.remaining = finite_timeout(active.timeout_millis);
                        active.deadline = active
                            .remaining
                            .filter(|duration| !duration.is_zero())
                            .and_then(|duration| Instant::now().checked_add(duration));
                        if !active.watch || failure == GeoFailure::PermissionDenied {
                            self.active.remove(&key);
                        }
                    }
                }
            }
        }
    }

    pub(super) fn drain_events(
        &mut self,
        router: &TabMessageRouter,
        window: Hwnd,
        visible_tab: Option<TabId>,
    ) {
        if self.events_overflow.swap(false, Ordering::AcqRel) {
            for (_, active) in self.active.drain() {
                active
                    .context
                    .emit(error(GeoFailure::PositionUnavailable), true);
            }
            self.events
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clear();
        }
        let mut all = self
            .events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut mine = Vec::new();
        let mut rest = VecDeque::new();
        while let Some(event) = all.pop_front() {
            let key = match &event {
                NativeEvent::Access(key, _) | NativeEvent::Position(key, _, _) => key,
            };
            if router.destination(key.tab) == Some(window as usize) {
                mine.push(event);
            } else if router.destination(key.tab).is_some() {
                rest.push_back(event);
            }
        }
        *all = rest;
        drop(all);
        for event in mine {
            self.process_event(event, router, visible_tab);
        }
    }

    pub(super) fn tick(
        &mut self,
        router: &TabMessageRouter,
        window: Hwnd,
        visible_tab: Option<TabId>,
    ) {
        self.drain_events(router, window, visible_tab);
        let now = Instant::now();
        let keys: Vec<_> = self
            .active
            .keys()
            .copied()
            .filter(|key| router.destination(key.tab) == Some(window as usize))
            .collect();
        for key in keys {
            let visible = visible_tab == Some(key.tab);
            let Some(active) = self.active.get_mut(&key) else {
                continue;
            };
            if !visible {
                if let Some(deadline) = active.deadline.take() {
                    active.remaining = Some(deadline.saturating_duration_since(now));
                }
                if active.source.take().is_some() {
                    active.generation = active.generation.wrapping_add(1);
                }
                continue;
            }
            if active.access_granted && active.source.is_none() {
                if active
                    .retry_after
                    .is_none_or(|retry_after| now >= retry_after)
                {
                    self.start_source(key, router);
                }
                continue;
            }
            if active.deadline.is_some_and(|deadline| now >= deadline) {
                active
                    .context
                    .emit(error(GeoFailure::Timeout), !active.watch);
                if active.watch {
                    // Repeated watch acquisition failures must not saturate the
                    // UI/IPC lane, including when the requested timeout is tiny.
                    active.remaining = (active.timeout_millis != u64::MAX)
                        .then(|| Duration::from_millis(active.timeout_millis.max(1000)));
                    active.deadline = active
                        .remaining
                        .and_then(|duration| now.checked_add(duration));
                } else {
                    self.active.remove(&key);
                }
            }
        }
    }
}

pub(super) fn error(failure: GeoFailure) -> GeolocationEvent {
    let (code, message) = match failure {
        GeoFailure::PermissionDenied => (
            GeolocationErrorCode::PermissionDenied,
            "Location permission was denied",
        ),
        GeoFailure::PositionUnavailable => (
            GeolocationErrorCode::PositionUnavailable,
            "Location is unavailable",
        ),
        GeoFailure::Timeout => (
            GeolocationErrorCode::Timeout,
            "Location acquisition timed out",
        ),
    };
    GeolocationEvent::Error {
        code,
        message: message.into(),
    }
}

pub(super) fn unix_time_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn position_is_current(position: &GeolocationPosition, started: u64, maximum_age: u64) -> bool {
    let now = unix_time_millis();
    if position.timestamp_millis > now {
        return false;
    }
    if maximum_age == 0 {
        // WinRT can return an older fix even when requested maximumAge is zero.
        // Millisecond timestamps permit a fix from the request's starting millisecond.
        position.timestamp_millis >= started
    } else {
        maximum_age == u64::MAX || now - position.timestamp_millis <= maximum_age
    }
}
