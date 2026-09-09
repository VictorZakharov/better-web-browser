//! Renderer readiness wakes; health polling remains a fallback, not task delivery.

use super::*;

pub(in crate::windows_app) const WM_APP_RENDERER_EVENTS: u32 = WM_APP + 18;
pub(super) const EVENTS_PER_TURN: usize = 32;

impl BrowserState {
    pub(super) fn watch_renderer_events(&self, id: TabId, session: &RendererSession) {
        let router = self.app.tab_router.clone();
        let session_id = session.snapshot().session_id;
        session.set_event_notifier(move || {
            if let Some(window) = router.destination(id) {
                // No pointer payload: a queued wake remains safe after tab closure,
                // detachment or renderer replacement. Failed posting is covered by
                // the health timer; do not block the broker or spin retrying.
                unsafe {
                    PostMessageW(
                        window as Hwnd,
                        WM_APP_RENDERER_EVENTS,
                        id.get() as usize,
                        session_id as isize,
                    );
                }
            }
        });
    }

    pub(in crate::windows_app) unsafe fn renderer_events_ready(
        &mut self,
        id: TabId,
        session_id: u64,
    ) {
        let current = self.tabs.get_mut(id).is_some_and(|tab| {
            tab.renderer_session
                .as_ref()
                .is_some_and(|session| session.snapshot().session_id == session_id)
        });
        if current {
            self.poll_renderer(id);
            // Backlogged batches use the low-priority 16 ms monitor timer. Posting
            // an endless continuation chain would starve native input and paint:
            // https://learn.microsoft.com/windows/win32/winmsg/about-messages-and-message-queues
            self.ensure_renderer_monitoring();
        }
    }
}
