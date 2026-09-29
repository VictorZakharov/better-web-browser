//! Browser-owned file selection. Paths stay in this process and never enter renderer IPC.
//!
//! The HTML accept attribute is a picker hint, not validation of selected files.
//! https://html.spec.whatwg.org/multipage/input.html#file-upload-state-(type=file)

mod accept;
mod native;
mod snapshot;

pub(super) use native::NativeFilePicker;

use super::app_state::BrowserState;
use super::platform::{GetForegroundWindow, Hwnd, IsIconic, IsWindowVisible};
use super::tabs::TabId;
use better_web_browser::fetch::{Origin, RequestClient};
use better_web_browser::renderer_process::FilePickerUpdateSink;
use better_web_browser::renderer_protocol::{
    DocumentId, DocumentNodeId, FilePickerRequest, FilePickerUpdate, MAX_FILE_PICKER_CHUNK_BYTES,
};
use std::path::PathBuf;

pub(super) trait FilePickerBackend {
    /// None means the user canceled. The returned paths must never leave the browser.
    fn select(&self, owner: Hwnd, multiple: bool, accept: &str)
    -> Result<Option<Vec<PathBuf>>, ()>;
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct PickerIdentity {
    tab: TabId,
    document: DocumentId,
    session: u64,
    request: u64,
    node: DocumentNodeId,
}

impl PickerIdentity {
    fn for_request(tab: TabId, request: &FilePickerRequest, session: u64) -> Self {
        Self {
            tab,
            document: request.document,
            session,
            request: request.request_id,
            node: request.node,
        }
    }
}

impl BrowserState {
    pub(in crate::windows_app) fn handle_file_picker_request(
        &mut self,
        tab_id: TabId,
        request: FilePickerRequest,
    ) {
        if request.validate().is_err() {
            return;
        }
        let Some((sink, session, origin)) = self.tabs.get_mut(tab_id).and_then(|tab| {
            if !tab.navigation.owns_document(request.document) {
                return None;
            }
            let renderer = tab.renderer_session.as_ref()?;
            let origin = tab
                .renderer_fetches
                .resolve_client(request.document, &tab.reader_url, request.client)
                .ok()
                .map(|owner| owner.origin);
            Some((
                renderer.file_picker_update_sink(request.document),
                renderer.snapshot().session_id,
                origin,
            ))
        }) else {
            return;
        };
        let identity = PickerIdentity::for_request(tab_id, &request, session);
        let Some(origin) = origin else {
            send_failed(&sink, &request);
            return;
        };
        if self.file_picker_open.is_some()
            || request.client != RequestClient::default()
            || !self.file_picker_request_is_current(identity, &request, &origin)
            || !self.consume_transient_activation(tab_id, request.document)
        {
            send_failed(&sink, &request);
            return;
        }

        // Show pumps a nested Windows message loop. Pin the exact request before entering it.
        self.file_picker_open = Some(identity);
        let selection =
            self.file_picker_backend
                .select(self.window, request.multiple, &request.accept);
        let still_open = self.file_picker_open == Some(identity);
        self.file_picker_open = None;
        if !still_open || !self.file_picker_request_is_current(identity, &request, &origin) {
            return;
        }
        let paths = match selection {
            Ok(Some(paths)) => paths,
            Ok(None) => {
                let _ = sink.try_send(FilePickerUpdate::Canceled {
                    document: request.document,
                    request_id: request.request_id,
                });
                return;
            }
            Err(()) => {
                send_failed(&sink, &request);
                return;
            }
        };
        let files = match snapshot::read_selected(&paths, request.multiple) {
            Ok(files) => files,
            Err(()) => {
                send_failed(&sink, &request);
                unsafe {
                    self.set_status(
                        "File selection could not be read; choose fewer or smaller files.",
                    );
                }
                return;
            }
        };
        if !self.file_picker_request_is_current(identity, &request, &origin) {
            return;
        }
        let metadata = files.iter().map(|file| file.metadata.clone()).collect();
        if sink
            .try_send(FilePickerUpdate::Start {
                document: request.document,
                request_id: request.request_id,
                files: metadata,
            })
            .is_err()
        {
            return;
        }
        for (index, file) in files.iter().enumerate() {
            for (chunk_index, bytes) in file.bytes.chunks(MAX_FILE_PICKER_CHUNK_BYTES).enumerate() {
                if sink
                    .try_send(FilePickerUpdate::Chunk {
                        document: request.document,
                        request_id: request.request_id,
                        file_index: index as u8,
                        offset: (chunk_index * MAX_FILE_PICKER_CHUNK_BYTES) as u32,
                        bytes: bytes.to_vec(),
                    })
                    .is_err()
                {
                    return;
                }
            }
        }
        let _ = sink.try_send(FilePickerUpdate::End {
            document: request.document,
            request_id: request.request_id,
        });
    }

    fn file_picker_request_is_current(
        &mut self,
        identity: PickerIdentity,
        request: &FilePickerRequest,
        origin: &Origin,
    ) -> bool {
        if !may_show_picker(
            self.benchmark.is_some(),
            unsafe { IsWindowVisible(self.window) != 0 },
            unsafe { IsIconic(self.window) != 0 },
            unsafe { GetForegroundWindow() == self.window },
        ) || self.tabs.active_id() != identity.tab
        {
            return false;
        }
        self.tabs.get_mut(identity.tab).is_some_and(|tab| {
            request.request_id == identity.request
                && request.node == identity.node
                && tab.navigation.owns_document(identity.document)
                && tab
                    .renderer_session
                    .as_ref()
                    .is_some_and(|renderer| renderer.snapshot().session_id == identity.session)
                && tab
                    .renderer_fetches
                    .resolve_client(request.document, &tab.reader_url, request.client)
                    .is_ok_and(|client| client.origin == *origin)
        })
    }
}

fn may_show_picker(benchmark: bool, visible: bool, iconic: bool, foreground: bool) -> bool {
    !benchmark && visible && !iconic && foreground
}

fn send_failed(sink: &FilePickerUpdateSink, request: &FilePickerRequest) {
    let _ = sink.try_send(FilePickerUpdate::Failed {
        document: request.document,
        request_id: request.request_id,
    });
}

#[cfg(test)]
mod tests {
    use super::may_show_picker;

    #[test]
    fn hidden_automation_cannot_show_native_dialog() {
        assert!(!may_show_picker(true, true, false, true));
        assert!(!may_show_picker(false, false, false, true));
        assert!(!may_show_picker(false, true, true, true));
        assert!(!may_show_picker(false, true, false, false));
        assert!(may_show_picker(false, true, false, true));
    }
}
