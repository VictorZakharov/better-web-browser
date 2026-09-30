//! One foreground document's earliest native runtime wakeup, owned by its HWND.

use super::*;
use crate::windows_app::tabs::TabId;
use better_web_browser::renderer_protocol::DocumentId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Wakeup {
    pub(super) owner: (TabId, DocumentId),
    pub(super) deadline: Instant,
}

#[derive(Default)]
pub(in crate::windows_app) struct RuntimeWakeup {
    installed: Option<Wakeup>,
}

impl RuntimeWakeup {
    pub(super) fn update(
        &mut self,
        requested: Option<Wakeup>,
        now: Instant,
        mut install: impl FnMut(Duration) -> bool,
        mut stop: impl FnMut(),
    ) -> bool {
        if let Some(requested) = requested {
            if self.installed.is_some_and(|existing| {
                existing.owner == requested.owner && existing.deadline <= requested.deadline
            }) {
                // SetTimer resets an existing timer. Later input must not postpone
                // an already promised rendering opportunity or JavaScript task.
                // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-settimer
                return true;
            }
            if !install(requested.deadline.saturating_duration_since(now)) {
                return false;
            }
        } else {
            stop();
        }
        self.installed = requested;
        true
    }

    pub(super) fn due(&self, owner: Option<(TabId, DocumentId)>, now: Instant) -> bool {
        self.installed
            .is_some_and(|wake| Some(wake.owner) == owner && now >= wake.deadline)
    }
}

#[cfg(test)]
mod tests;
