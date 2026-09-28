//! UI-thread admission for renderer Clipboard intents.

use super::*;
use crate::windows_app::{
    app_state::BrowserState,
    platform::{GetForegroundWindow, IsIconic, IsWindowVisible, MessageBoxW},
    tabs::TabId,
    win32_helpers::wide,
};
use better_web_browser::renderer_process::ClipboardUpdateSink;
use better_web_browser::renderer_protocol::{
    ClipboardAction, ClipboardRequest, ClipboardUpdate, ClipboardValue,
};

impl BrowserState {
    pub(in crate::windows_app) fn handle_clipboard_request(
        &mut self,
        tab_id: TabId,
        request: ClipboardRequest,
    ) {
        if request.validate().is_err() {
            return;
        }
        let Some((sink, session_id, owner)) = self.tabs.get_mut(tab_id).and_then(|tab| {
            if !tab.navigation.owns_document(request.document) {
                return None;
            }
            let session = tab.renderer_session.as_ref()?;
            let owner = tab.renderer_fetches.resolve_client(
                request.document,
                &tab.reader_url,
                request.client,
            );
            Some((
                session.clipboard_update_sink(request.document),
                session.snapshot().session_id,
                owner,
            ))
        }) else {
            return;
        };
        let Ok(owner) = owner else {
            reject(&sink, &request, ClipboardError::NotAllowed);
            return;
        };
        // The current activation ledger is tab/document scoped, not frame scoped.
        // A descendant cannot borrow a top-level click to access host clipboard.
        if !top_level_clipboard_client_eligible(request.client)
            || !clipboard_origin_eligible(&owner.origin)
            || !self.request_is_current(tab_id, &request, session_id, &owner.origin)
            || !self.consume_transient_activation(tab_id, request.document)
        {
            reject(&sink, &request, ClipboardError::NotAllowed);
            return;
        }
        let access = match request.action {
            ClipboardAction::ReadText => Access::Read,
            ClipboardAction::WriteText(_) => Access::Write,
        };
        let decision = self.app.clipboard.borrow().decision(&owner.origin, access);
        let allowed = if let Some(decision) = decision {
            decision
        } else {
            let action = match access {
                Access::Read => "read text from",
                Access::Write => "write text to",
            };
            let question = wide(&format!(
                "Allow {} to {action} your system clipboard for this browser session?",
                owner.origin.serialize()
            ));
            let title = wide("Breeze clipboard permission");
            // MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2: denial is the default.
            let accepted =
                unsafe { MessageBoxW(self.window, question.as_ptr(), title.as_ptr(), 0x124) == 6 };
            // MessageBox pumps a nested UI loop. Navigation, tab selection or
            // renderer replacement can happen while the prompt is open.
            if !self.request_is_current(tab_id, &request, session_id, &owner.origin) {
                reject(&sink, &request, ClipboardError::NotAllowed);
                return;
            }
            self.app
                .clipboard
                .borrow_mut()
                .decide(owner.origin.clone(), access, accepted);
            accepted
        };
        if !allowed || !self.request_is_current(tab_id, &request, session_id, &owner.origin) {
            reject(&sink, &request, ClipboardError::NotAllowed);
            return;
        }
        let result = match &request.action {
            ClipboardAction::ReadText => self
                .app
                .clipboard
                .borrow()
                .read_text(self.window)
                .map(ClipboardValue::Text),
            ClipboardAction::WriteText(text) => self
                .app
                .clipboard
                .borrow()
                .write_text(self.window, text)
                .map(|()| ClipboardValue::Written),
        };
        let _ = sink.try_send(ClipboardUpdate {
            document: request.document,
            request_id: request.request_id,
            result,
        });
    }

    fn request_is_current(
        &mut self,
        tab_id: TabId,
        request: &ClipboardRequest,
        session_id: u64,
        origin: &Origin,
    ) -> bool {
        if self.benchmark.is_some()
            || self.tabs.active_id() != tab_id
            || unsafe {
                IsWindowVisible(self.window) == 0
                    || IsIconic(self.window) != 0
                    || GetForegroundWindow() != self.window
            }
        {
            return false;
        }
        self.tabs.get_mut(tab_id).is_some_and(|tab| {
            tab.navigation.owns_document(request.document)
                && tab
                    .renderer_session
                    .as_ref()
                    .is_some_and(|session| session.snapshot().session_id == session_id)
                && tab
                    .renderer_fetches
                    .resolve_client(request.document, &tab.reader_url, request.client)
                    .is_ok_and(|client| client.origin == *origin)
        })
    }
}

fn reject(sink: &ClipboardUpdateSink, request: &ClipboardRequest, error: ClipboardError) {
    let _ = sink.try_send(ClipboardUpdate {
        document: request.document,
        request_id: request.request_id,
        result: Err(error),
    });
}

fn top_level_clipboard_client_eligible(client: better_web_browser::fetch::RequestClient) -> bool {
    client.id == 0 && !client.opaque
}

fn clipboard_origin_eligible(origin: &Origin) -> bool {
    // Match the SecureContext exposure in the renderer, including loopback HTTP.
    origin.is_potentially_trustworthy()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_is_restricted_to_top_level_client() {
        // The tab-wide activation ledger cannot safely authorize a child frame.
        assert!(top_level_clipboard_client_eligible(
            better_web_browser::fetch::RequestClient {
                id: 0,
                opaque: false
            }
        ));
        assert!(!top_level_clipboard_client_eligible(
            better_web_browser::fetch::RequestClient {
                id: 1,
                opaque: false
            }
        ));
        assert!(!top_level_clipboard_client_eligible(
            better_web_browser::fetch::RequestClient {
                id: 0,
                opaque: true
            }
        ));
    }

    #[test]
    fn origin_admission_matches_secure_context_exposure() {
        for url in [
            "https://example.test/",
            "http://localhost:8080/",
            "http://127.0.0.1:8080/",
            "http://[::1]:8080/",
        ] {
            assert!(
                clipboard_origin_eligible(&Origin::parse(url).unwrap()),
                "{url}"
            );
        }
        assert!(!clipboard_origin_eligible(
            &Origin::parse("http://example.test/").unwrap()
        ));
    }
}
