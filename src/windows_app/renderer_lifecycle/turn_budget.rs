//! A renderer output turn yields between atomic events, never inside a default action.

use super::*;
use better_web_browser::renderer_process::RendererEvent;
use std::time::{Duration, Instant};

const OUTPUT_TURN_BUDGET: Duration = Duration::from_millis(4);

pub(super) struct RendererTurnBudget {
    started: Instant,
}

impl RendererTurnBudget {
    pub(super) fn new(started: Instant) -> Self {
        Self { started }
    }

    // Only call after one completed atomic event. Even a busy input queue or an
    // expensive first event therefore cannot starve renderer output completely.
    pub(super) fn should_yield(
        &self,
        has_remaining: bool,
        now: Instant,
        input_is_waiting: impl FnOnce() -> bool,
    ) -> bool {
        has_remaining
            && (now.saturating_duration_since(self.started) >= OUTPUT_TURN_BUDGET
                || input_is_waiting())
    }
}

fn finish_notification(has_deferred: bool, finish: impl FnOnce() -> bool) -> bool {
    has_deferred || finish()
}

impl BrowserState {
    pub(super) fn defer_renderer_event_turn(
        &mut self,
        id: TabId,
        session_id: u64,
        events: impl IntoIterator<Item = RendererEvent>,
    ) {
        if let Some(tab) = self.tabs.get_mut(id)
            && tab
                .renderer_session
                .as_ref()
                .is_some_and(|session| session.snapshot().session_id == session_id)
        {
            // Navigation can replace a session during its event. Never attach
            // the old session's remaining output to the replacement document.
            tab.deferred_renderer_events.extend(events);
        }
    }

    pub(super) fn finish_renderer_event_turn(
        &mut self,
        id: TabId,
        session_id: u64,
    ) -> Option<bool> {
        let tab = self.tabs.get_mut(id)?;
        let session = tab
            .renderer_session
            .as_ref()
            .filter(|session| session.snapshot().session_id == session_id)?;
        // The broker may be empty while this window owns the unprocessed tail.
        // Keep its wake marked pending until both queues drain: new arrivals must
        // not create a posted-message continuation chain ahead of native input.
        // The existing monitor sees deferred work and resumes it at low priority.
        Some(finish_notification(
            !tab.deferred_renderer_events.is_empty(),
            || session.finish_event_drain(),
        ))
    }
}

#[cfg(test)]
mod tests;
