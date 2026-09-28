//! Browser-owned consent and dispatch for HTML custom scheme handlers.

use super::*;
use better_web_browser::protocol_handlers::{Handler, Registry, normalize};
use better_web_browser::renderer_protocol::{ProtocolHandlerAction, ProtocolHandlerRequest};
use windows_sys::Win32::UI::WindowsAndMessaging::IsIconic;

impl BrowserState {
    pub(super) fn handle_protocol_handler_request(
        &mut self,
        tab_id: tabs::TabId,
        request: ProtocolHandlerRequest,
    ) {
        if request.validate().is_err() || request.client.id != 0 || request.client.opaque {
            return;
        }
        let Some((session_id, owner)) = self.tabs.get_mut(tab_id).and_then(|tab| {
            if !tab.navigation.owns_document(request.document) {
                return None;
            }
            let session_id = tab.renderer_session.as_ref()?.snapshot().session_id;
            let owner = tab
                .renderer_fetches
                .resolve_client(request.document, &tab.reader_url, request.client)
                .ok()?;
            Some((session_id, owner))
        }) else {
            return;
        };
        if !owner.origin.is_potentially_trustworthy() {
            return;
        }
        let Ok(handler) = normalize(
            &request.scheme,
            &request.template,
            &owner.url,
            &owner.origin,
        ) else {
            return;
        };
        if request.action == ProtocolHandlerAction::Unregister {
            let mut next = self.app.protocol_handlers.borrow().clone();
            if next.unregister(&handler) {
                self.persist_protocol_handlers(next);
            }
            return;
        }
        {
            let registry = self.app.protocol_handlers.borrow();
            if registry.is_approved(&handler) || registry.was_denied(&handler) {
                return;
            }
        }
        // Registration never silently becomes a default. Only a visible, active,
        // directly activated top-level document can open this native prompt.
        if self.benchmark.is_some()
            || self.tabs.active_id() != tab_id
            || !self.has_transient_activation(tab_id, request.document)
            || unsafe { IsWindowVisible(self.window) } == 0
            || unsafe { IsIconic(self.window) } != 0
            || unsafe { GetForegroundWindow() } != self.window
            || !self.consume_transient_activation(tab_id, request.document)
        {
            return;
        }
        let prompt = wide(&format!(
            "Allow {} to handle {}: links in Breeze?\n\nHandler: {}",
            handler.origin,
            handler.scheme,
            prompt_template(&handler.template),
        ));
        let title = wide("Breeze protocol-handler request");
        // MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2: deny is the default.
        let choice = unsafe { MessageBoxW(self.window, prompt.as_ptr(), title.as_ptr(), 0x124) };
        if !matches!(choice, 6 | 7) {
            return;
        }
        let approved = choice == 6;
        // MessageBox runs a nested UI loop. A navigation, renderer replacement, tab
        // switch or focus loss while it was open invalidates the old permission ask.
        if !self.protocol_prompt_still_owns_document(
            tab_id,
            request.document,
            session_id,
            &owner.origin,
            &handler,
        ) {
            return;
        }
        let mut next = self.app.protocol_handlers.borrow().clone();
        if approved {
            if !next.approve(handler) {
                unsafe {
                    self.set_status("Protocol-handler limit reached");
                }
                return;
            }
        } else {
            next.deny(handler);
        }
        self.persist_protocol_handlers(next);
    }

    fn protocol_prompt_still_owns_document(
        &mut self,
        tab_id: tabs::TabId,
        document: better_web_browser::renderer_protocol::DocumentId,
        session_id: u64,
        origin: &better_web_browser::fetch::Origin,
        handler: &Handler,
    ) -> bool {
        if self.tabs.active_id() != tab_id
            || unsafe { IsWindowVisible(self.window) } == 0
            || unsafe { IsIconic(self.window) } != 0
            || unsafe { GetForegroundWindow() } != self.window
        {
            return false;
        }
        self.tabs.get_mut(tab_id).is_some_and(|tab| {
            if !tab.navigation.owns_document(document)
                || tab
                    .renderer_session
                    .as_ref()
                    .map(|session| session.snapshot().session_id)
                    != Some(session_id)
            {
                return false;
            }
            let Ok(owner) = tab.renderer_fetches.resolve_client(
                document,
                &tab.reader_url,
                better_web_browser::fetch::RequestClient {
                    id: 0,
                    opaque: false,
                },
            ) else {
                return false;
            };
            owner.origin.is_same_origin(origin)
                && normalize(
                    &handler.scheme,
                    &handler.template,
                    &owner.url,
                    &owner.origin,
                )
                .is_ok_and(|current| current == *handler)
        })
    }

    fn persist_protocol_handlers(&mut self, next: Registry) {
        match next.save(&self.app.profile) {
            Ok(()) => *self.app.protocol_handlers.borrow_mut() = next,
            Err(error) => unsafe {
                self.set_status(&format!("Protocol handler not saved: {error}"));
            },
        }
    }
}

fn prompt_template(template: &str) -> String {
    const MAX_VISIBLE_CHARS: usize = 256;
    let mut label = String::new();
    for (index, character) in template.chars().enumerate() {
        if index == MAX_VISIBLE_CHARS {
            label.push_str("… (truncated)");
            break;
        }
        label.push(if character.is_control() {
            '�'
        } else {
            character
        });
    }
    label
}

#[cfg(test)]
mod tests {
    use super::prompt_template;

    #[test]
    fn native_prompt_sanitizes_controls_and_bounds_untrusted_template() {
        assert_eq!(
            prompt_template("https://site.test/a\n?uri=%s"),
            "https://site.test/a�?uri=%s"
        );
        let label = prompt_template(&"x".repeat(2048));
        assert_eq!(label.chars().count(), 269);
        assert!(label.ends_with("… (truncated)"));
    }
}
