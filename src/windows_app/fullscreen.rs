//! Native fullscreen ownership and restoration for browser and page requests.

use super::tabs::TabId;
use super::*;
use better_web_browser::renderer_protocol::{
    DocumentId, FullscreenAction, FullscreenDisposition, FullscreenRequest, FullscreenResponse,
};

const TRANSIENT_ACTIVATION_LIFETIME: Duration = Duration::from_secs(5);

#[derive(Clone, Copy)]
pub(super) enum FullscreenOwner {
    Browser,
    Page { tab: TabId, document: DocumentId },
}

#[derive(Clone, Copy)]
struct SavedWindowState {
    style: isize,
    extended_style: isize,
    placement: WindowPlacement,
    focused: Hwnd,
}

#[derive(Default)]
pub(super) struct FullscreenState {
    owner: Option<FullscreenOwner>,
    saved: Option<SavedWindowState>,
}

impl FullscreenState {
    pub(super) fn is_active(&self) -> bool {
        self.owner.is_some()
    }
}

impl BrowserState {
    pub(super) unsafe fn toggle_browser_fullscreen(&mut self) {
        if self.fullscreen.is_active() {
            self.exit_fullscreen(true);
        } else {
            self.enter_fullscreen(FullscreenOwner::Browser);
        }
    }

    pub(super) unsafe fn handle_fullscreen_request(
        &mut self,
        tab_id: TabId,
        request: FullscreenRequest,
    ) {
        let owns_document = self.tabs.active_id() == tab_id
            && self
                .tabs
                .get_mut(tab_id)
                .is_some_and(|tab| tab.navigation.owns_document(request.document));
        let disposition = match request.action {
            FullscreenAction::Enter
                if owns_document && self.consume_transient_activation(tab_id, request.document) =>
            {
                if self.fullscreen.is_active() {
                    self.exit_fullscreen(false);
                }
                if self.enter_fullscreen(FullscreenOwner::Page {
                    tab: tab_id,
                    document: request.document,
                }) {
                    FullscreenDisposition::Entered
                } else {
                    FullscreenDisposition::Denied
                }
            }
            FullscreenAction::Exit
                if matches!(
                    self.fullscreen.owner,
                    Some(FullscreenOwner::Page { tab, document })
                        if tab == tab_id && document == request.document
                ) =>
            {
                self.exit_fullscreen(false);
                FullscreenDisposition::Exited
            }
            _ => FullscreenDisposition::Denied,
        };
        self.respond_fullscreen(
            tab_id,
            FullscreenResponse {
                document: request.document,
                request_id: request.request_id,
                disposition,
            },
        );
    }

    pub(super) unsafe fn exit_page_fullscreen(&mut self) -> bool {
        self.exit_page_fullscreen_for(None, true)
    }

    pub(super) unsafe fn abandon_page_fullscreen_owned_by(&mut self, owner_tab: TabId) -> bool {
        self.exit_page_fullscreen_for(Some(owner_tab), false)
    }

    unsafe fn exit_page_fullscreen_for(
        &mut self,
        owner_tab: Option<TabId>,
        notify_page: bool,
    ) -> bool {
        let Some(FullscreenOwner::Page { tab, document }) = self.fullscreen.owner else {
            return false;
        };
        if owner_tab.is_some_and(|owner_tab| owner_tab != tab) {
            return false;
        }
        self.exit_fullscreen(false);
        if notify_page {
            self.respond_fullscreen(
                tab,
                FullscreenResponse {
                    document,
                    request_id: 0,
                    disposition: FullscreenDisposition::Exited,
                },
            );
        }
        true
    }

    fn consume_transient_activation(&mut self, tab_id: TabId, document: DocumentId) -> bool {
        self.tabs.get_mut(tab_id).is_some_and(|tab| {
            tab.transient_activation
                .take()
                .is_some_and(|activation| activation_authorizes(document, activation))
        })
    }

    fn respond_fullscreen(&mut self, tab_id: TabId, response: FullscreenResponse) {
        let result = self
            .tabs
            .get_mut(tab_id)
            .and_then(|tab| tab.renderer_session.as_ref())
            .map(|session| session.respond_fullscreen(response));
        if let Some(Err(error)) = result {
            unsafe {
                self.contain_page_engine_failure(
                    tab_id,
                    format!("could not acknowledge fullscreen request: {error}"),
                );
            }
        }
    }

    unsafe fn enter_fullscreen(&mut self, owner: FullscreenOwner) -> bool {
        let mut placement = WindowPlacement {
            length: size_of::<WindowPlacement>() as u32,
            ..WindowPlacement::default()
        };
        let style = GetWindowLongPtrW(self.window, GWL_STYLE);
        let extended_style = GetWindowLongPtrW(self.window, GWL_EXSTYLE);
        if GetWindowPlacement(self.window, &mut placement) == 0 {
            return false;
        }
        let monitor = MonitorFromWindow(self.window, MONITOR_DEFAULTTONEAREST);
        let mut information = MonitorInfo {
            size: size_of::<MonitorInfo>() as u32,
            ..MonitorInfo::default()
        };
        if monitor.is_null() || GetMonitorInfoW(monitor, &mut information) == 0 {
            return false;
        }
        self.fullscreen.saved = Some(SavedWindowState {
            style,
            extended_style,
            placement,
            focused: GetFocus(),
        });
        self.fullscreen.owner = Some(owner);
        let fullscreen_style = ((style as u32 & !WS_OVERLAPPEDWINDOW) | WS_POPUP) as isize;
        SetWindowLongPtrW(self.window, GWL_STYLE, fullscreen_style);
        self.set_chrome_controls_visible(false);
        let rectangle = information.monitor;
        if SetWindowPos(
            self.window,
            null_mut(),
            rectangle.left,
            rectangle.top,
            rectangle.width(),
            rectangle.height(),
            SWP_NOZORDER | SWP_FRAMECHANGED,
        ) == 0
        {
            self.exit_fullscreen(false);
            return false;
        }
        self.finish_fullscreen_layout();
        SetFocus(self.window);
        true
    }

    unsafe fn exit_fullscreen(&mut self, notify_page: bool) {
        let owner = self.fullscreen.owner.take();
        let Some(saved) = self.fullscreen.saved.take() else {
            return;
        };
        SetWindowLongPtrW(self.window, GWL_STYLE, saved.style);
        SetWindowLongPtrW(self.window, GWL_EXSTYLE, saved.extended_style);
        let normal = saved.placement.normal_position;
        SetWindowPos(
            self.window,
            null_mut(),
            normal.left,
            normal.top,
            normal.width(),
            normal.height(),
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        SetWindowPlacement(self.window, &saved.placement);
        self.set_chrome_controls_visible(true);
        self.finish_fullscreen_layout();
        SetFocus(if saved.focused.is_null() {
            self.window
        } else {
            saved.focused
        });
        if notify_page && let Some(FullscreenOwner::Page { tab, document }) = owner {
            self.respond_fullscreen(
                tab,
                FullscreenResponse {
                    document,
                    request_id: 0,
                    disposition: FullscreenDisposition::Exited,
                },
            );
        }
    }

    unsafe fn finish_fullscreen_layout(&mut self) {
        self.reset_media_viewport_width();
        self.resize_controls();
        self.rebuild_layout();
        self.sync_page_control_positions();
        InvalidateRect(self.window, null(), 1);
    }

    unsafe fn set_chrome_controls_visible(&self, visible: bool) {
        let command = if visible { SW_SHOW } else { SW_HIDE };
        for control in [
            self.controls.back,
            self.controls.forward,
            self.controls.reload,
            self.controls.address,
            self.controls.go,
            self.controls.task_manager,
            self.controls.reader,
        ] {
            ShowWindow(control, command);
        }
        if !self.performance_window.is_null() {
            ShowWindow(
                self.performance_window,
                if visible && self.performance_panel_visible {
                    SW_SHOW
                } else {
                    SW_HIDE
                },
            );
        }
    }
}

fn activation_authorizes(document: DocumentId, activation: (DocumentId, Instant)) -> bool {
    activation.0 == document && activation.1.elapsed() <= TRANSIENT_ACTIVATION_LIFETIME
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_activation_is_document_scoped_and_expires() {
        let first = DocumentId::new(1).unwrap();
        let replacement = DocumentId::new(2).unwrap();
        assert!(activation_authorizes(first, (first, Instant::now())));
        assert!(!activation_authorizes(replacement, (first, Instant::now())));
        assert!(!activation_authorizes(
            first,
            (
                first,
                Instant::now() - TRANSIENT_ACTIVATION_LIFETIME - Duration::from_millis(1)
            )
        ));
    }
}
