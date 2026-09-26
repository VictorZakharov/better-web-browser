//! Trusted tab/document admission for requests from the sandboxed renderer.

use super::SpeechOwner;
use crate::windows_app::app_state::BrowserState;
use crate::windows_app::tabs::TabId;
use better_web_browser::renderer_protocol::{
    SpeechAction, SpeechEvent, SpeechRequest, SpeechUpdate,
};

impl BrowserState {
    pub(in crate::windows_app) fn retire_speech_for_window(&mut self) {
        let ids = self.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>();
        for id in ids {
            self.retire_speech_for_tab(id);
        }
    }

    pub(in crate::windows_app) fn retire_speech_for_tab(&mut self, tab_id: TabId) {
        if let Some((document, session_id)) = self.tabs.get_mut(tab_id).and_then(|tab| {
            let session_id = tab.renderer_session.as_ref()?.snapshot().session_id;
            let document = tab
                .navigation
                .active_document()
                .or_else(|| tab.navigation.document_id().ok())?;
            Some((document, session_id))
        }) {
            self.app.speech_service.retire(SpeechOwner {
                tab: tab_id,
                document,
                session_id,
            });
        }
    }

    pub(in crate::windows_app) fn handle_speech_request(
        &mut self,
        tab_id: TabId,
        request: SpeechRequest,
    ) {
        if request.validate().is_err() {
            return;
        }
        let document = request.document;
        let Some((sink, session_id)) = self.tabs.get_mut(tab_id).and_then(|tab| {
            (tab.navigation.owns_document(document))
                .then_some(tab.renderer_session.as_ref())
                .flatten()
                .map(|session| {
                    (
                        session.speech_update_sink(document),
                        session.snapshot().session_id,
                    )
                })
        }) else {
            return;
        };
        // The renderer can claim any action. Only the browser's input path can
        // grant transient activation. Multiple utterances may be queued in one
        // gesture, so speaking checks the short-lived state without consuming it.
        if matches!(request.action, SpeechAction::Speak { .. })
            && (self.tabs.active_id() != tab_id || !self.has_transient_activation(tab_id, document))
        {
            let _ = sink.try_send(SpeechUpdate {
                document,
                utterance_id: request.utterance_id,
                event: SpeechEvent::Error("not-allowed".into()),
            });
            return;
        }
        let utterance_id = request.utterance_id;
        let action_can_error = matches!(
            request.action,
            SpeechAction::Speak { .. } | SpeechAction::GetVoices
        );
        let owner = SpeechOwner {
            tab: tab_id,
            document,
            session_id,
        };
        if let Err(error) = self.app.speech_service.send(owner, request, sink.clone())
            && action_can_error
        {
            let _ = sink.try_send(SpeechUpdate {
                document,
                utterance_id,
                event: SpeechEvent::Error(error.chars().take(128).collect()),
            });
        }
    }
}
