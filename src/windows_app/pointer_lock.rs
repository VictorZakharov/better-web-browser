//! Native Pointer Lock ownership, cursor capture and browser-initiated release.

use super::tabs::TabId;
use super::*;
use better_web_browser::renderer_protocol::{
    DocumentId, DocumentNodeId, PointerLockDisposition, PointerLockRequest, PointerLockResponse,
};

const ACTIVATION_LIFETIME: Duration = Duration::from_secs(5);

#[derive(Clone, Copy)]
pub(super) struct PointerLockOwner {
    tab: TabId,
    document: DocumentId,
    target: DocumentNodeId,
    cursor_before: Point,
    anchor_client: Point,
    anchor_screen: Point,
}

#[derive(Default)]
pub(super) struct PointerLockState {
    pub(super) owner: Option<PointerLockOwner>,
    released_document: Option<DocumentId>,
}

impl BrowserState {
    pub(super) unsafe fn route_locked_pointer(
        &mut self,
        owner: PointerLockOwner,
        x: i32,
        y: i32,
        phase: better_web_browser::renderer_protocol::PointerPhase,
        button: better_web_browser::renderer_protocol::PointerButton,
        wparam: Wparam,
    ) -> bool {
        use better_web_browser::renderer_protocol::{DocumentInput, PointerInput, PointerPhase};
        let anchor = owner.anchor_client;
        let scale = self.page_scale().max(f32::EPSILON);
        let (phase, doc_x, doc_y) = if phase == PointerPhase::Move {
            let delta_x = x - anchor.x;
            let delta_y = y - anchor.y;
            // SetCursorPos generates a synthetic move at the anchor. It is not
            // user motion and must not reach the document.
            if delta_x == 0 && delta_y == 0 {
                return true;
            }
            SetCursorPos(owner.anchor_screen.x, owner.anchor_screen.y);
            (
                PointerPhase::LockedMove,
                delta_x as f32 / scale,
                delta_y as f32 / scale,
            )
        } else {
            (
                phase,
                anchor.x as f32 / scale,
                (anchor.y - self.toolbar_height() + self.scroll_y) as f32 / scale,
            )
        };
        let Some((document, sequence)) = self.next_renderer_input() else {
            return false;
        };
        if document != owner.document {
            return false;
        }
        let accepted = self.submit_renderer_input(DocumentInput::Pointer(PointerInput {
            document,
            sequence,
            phase,
            button,
            buttons: renderer_input::current_buttons(),
            x: doc_x,
            y: doc_y,
            modifiers: renderer_input::pointer_modifiers(wparam),
            target: Some(owner.target),
        }));
        if accepted
            && phase == PointerPhase::Up
            && button == better_web_browser::renderer_protocol::PointerButton::Primary
        {
            self.transient_activation = Some((document, Instant::now()));
        }
        accepted
    }

    pub(super) unsafe fn handle_pointer_lock_request(
        &mut self,
        tab_id: TabId,
        request: PointerLockRequest,
    ) {
        let owns_document = self.tabs.active_id() == tab_id
            && self
                .tabs
                .get_mut(tab_id)
                .is_some_and(|tab| tab.navigation.owns_document(request.document));
        let disposition = if !owns_document {
            PointerLockDisposition::Denied
        } else if let Some(target) = request.target {
            if self.acquire_pointer_lock(tab_id, request.document, target) {
                PointerLockDisposition::Entered
            } else {
                PointerLockDisposition::Denied
            }
        } else if self.release_pointer_lock_for(Some(tab_id), false) {
            self.pointer_lock.released_document = Some(request.document);
            PointerLockDisposition::Exited
        } else {
            PointerLockDisposition::Denied
        };
        self.respond_pointer_lock(
            tab_id,
            PointerLockResponse {
                document: request.document,
                request_id: request.request_id,
                disposition,
            },
        );
    }

    unsafe fn acquire_pointer_lock(
        &mut self,
        tab_id: TabId,
        document: DocumentId,
        target: DocumentNodeId,
    ) -> bool {
        if let Some(owner) = self.pointer_lock.owner {
            if owner.tab != tab_id || owner.document != document {
                return false;
            }
            self.pointer_lock.owner = Some(PointerLockOwner { target, ..owner });
            return true;
        }
        let activated = self.tabs.get_mut(tab_id).is_some_and(|tab| {
            tab.transient_activation
                .is_some_and(|(id, when)| id == document && when.elapsed() <= ACTIVATION_LIFETIME)
        });
        if !activated && self.pointer_lock.released_document != Some(document) {
            return false;
        }
        if self.surface != Surface::Page
            || self.window.is_null()
            || IsWindowVisible(self.window) == 0
            || GetForegroundWindow() != self.window
        {
            return false;
        }
        let mut before = Point { x: 0, y: 0 };
        let mut client: Rect = std::mem::zeroed();
        if GetCursorPos(&mut before) == 0 || GetClientRect(self.window, &mut client) == 0 {
            return false;
        }
        let toolbar = self.toolbar_height();
        let bottom = (toolbar + self.viewport_height()).min(client.bottom);
        if bottom <= toolbar || client.right <= 0 {
            return false;
        }
        let anchor_client = Point {
            x: client.right / 2,
            y: toolbar + (bottom - toolbar) / 2,
        };
        let mut upper = Point { x: 0, y: toolbar };
        let mut lower = Point {
            x: client.right - 1,
            y: bottom - 1,
        };
        let mut anchor_screen = anchor_client;
        if ClientToScreen(self.window, &mut upper) == 0
            || ClientToScreen(self.window, &mut lower) == 0
            || ClientToScreen(self.window, &mut anchor_screen) == 0
        {
            return false;
        }
        // Clip to content, not the whole screen. Recentring each real move
        // yields unbounded relative motion without stealing clicks in chrome.
        let clip = Rect {
            left: upper.x,
            top: upper.y,
            right: lower.x + 1,
            bottom: lower.y + 1,
        };
        if ClipCursor(&clip) == 0 {
            return false;
        }
        if SetCursorPos(anchor_screen.x, anchor_screen.y) == 0 {
            ClipCursor(null());
            return false;
        }
        self.pointer_lock.owner = Some(PointerLockOwner {
            tab: tab_id,
            document,
            target,
            cursor_before: before,
            anchor_client,
            anchor_screen,
        });
        SetCursor(null_mut());
        true
    }

    pub(super) unsafe fn exit_pointer_lock(&mut self) -> bool {
        self.release_pointer_lock_for(None, true)
    }

    pub(super) unsafe fn abandon_pointer_lock_owned_by(&mut self, tab: TabId) -> bool {
        self.release_pointer_lock_for(Some(tab), false)
    }

    pub(super) unsafe fn release_pointer_lock(&mut self, notify_page: bool) -> bool {
        self.release_pointer_lock_for(None, notify_page)
    }

    unsafe fn release_pointer_lock_for(&mut self, tab: Option<TabId>, notify_page: bool) -> bool {
        let Some(owner) = self.pointer_lock.owner else {
            return false;
        };
        if tab.is_some_and(|tab| tab != owner.tab) {
            return false;
        }
        self.pointer_lock.owner = None;
        ClipCursor(null());
        SetCursorPos(owner.cursor_before.x, owner.cursor_before.y);
        self.apply_current_pointer_cursor();
        if notify_page {
            self.respond_pointer_lock(
                owner.tab,
                PointerLockResponse {
                    document: owner.document,
                    request_id: 0,
                    disposition: PointerLockDisposition::Exited,
                },
            );
        }
        true
    }

    fn respond_pointer_lock(&mut self, tab: TabId, response: PointerLockResponse) {
        let result = self
            .tabs
            .get_mut(tab)
            .and_then(|tab| tab.renderer_session.as_ref())
            .map(|session| session.respond_pointer_lock(response));
        if let Some(Err(error)) = result {
            unsafe {
                self.contain_page_engine_failure(
                    tab,
                    format!("could not acknowledge pointer lock request: {error}"),
                );
            }
        }
    }

    pub(super) fn locked_pointer_owner(&self) -> Option<PointerLockOwner> {
        self.pointer_lock.owner.filter(|owner| {
            self.tabs.active_id() == owner.tab
                && self.navigation.active_document() == Some(owner.document)
        })
    }
}

impl PointerLockOwner {
    pub(super) fn target(self) -> DocumentNodeId {
        self.target
    }
    pub(super) fn anchor_client(self) -> Point {
        self.anchor_client
    }
}
