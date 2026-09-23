//! Browser-owned WebSocket workers, bounded per document and retired on navigation.

use super::super::{BrowserState, tabs::TabId};
use super::RendererFetchClient;
use better_web_browser::fetch::{FetchSignal, RequestClient};
use better_web_browser::renderer_process::WebSocketEventSink;
use better_web_browser::renderer_protocol::{
    DocumentId, WebSocketCommand, WebSocketEvent, WebSocketEventKind, WebSocketOperation,
};
use better_web_browser::winhttp::{HttpClient, WebSocketConnection, WebSocketFrame};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

const MAX_SOCKETS_PER_DOCUMENT: usize = 8;
const COMMAND_QUEUE: usize = 32;

impl BrowserState {
    pub(in crate::windows_app) fn handle_websocket_command(
        &mut self,
        id: TabId,
        command: WebSocketCommand,
    ) {
        let context = self.tabs.get_mut(id).and_then(|tab| {
            tab.navigation
                .owns_document(command.document)
                .then(|| {
                    tab.renderer_session.as_ref().map(|session| {
                        (
                            tab.reader_url.clone(),
                            tab.document_fetch.signal(),
                            session.websocket_event_sink(command.document),
                            tab.renderer_websockets.clone(),
                            tab.renderer_fetches.clone(),
                        )
                    })
                })
                .flatten()
        });
        if let Some((document_url, signal, sink, registry, clients)) = context {
            match clients.resolve_client(command.document, &document_url, command.client) {
                Ok(owner) => {
                    registry.command(command, owner, Arc::clone(&self.http_client), signal, sink)
                }
                Err(_) => reject(command.document, command.socket_id, &sink),
            }
        }
    }
}

#[derive(Clone, Default)]
pub(in crate::windows_app) struct RendererWebSocketRegistry {
    entries: Arc<Mutex<HashMap<(DocumentId, u64), SocketEntry>>>,
}

struct SocketEntry {
    commands: mpsc::SyncSender<WebSocketOperation>,
    canceled: Arc<AtomicBool>,
    client: RequestClient,
}

impl RendererWebSocketRegistry {
    pub(in crate::windows_app) fn cancel_all(&self) {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for (_, entry) in entries.drain() {
            entry.canceled.store(true, Ordering::Release);
            let _ = entry.commands.try_send(WebSocketOperation::Close {
                code: 1001,
                reason: String::new(),
            });
        }
    }

    pub(super) fn command(
        &self,
        command: WebSocketCommand,
        owner: RendererFetchClient,
        client: Arc<HttpClient>,
        signal: FetchSignal,
        sink: WebSocketEventSink,
    ) {
        let key = (command.document, command.socket_id);
        match command.operation {
            WebSocketOperation::Open { url, protocols } => {
                let (sender, receiver) = mpsc::sync_channel(COMMAND_QUEUE);
                let canceled = Arc::new(AtomicBool::new(false));
                let admitted = {
                    let mut entries = self
                        .entries
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    if entries.contains_key(&key)
                        || entries
                            .keys()
                            .filter(|(document, _)| *document == command.document)
                            .count()
                            >= MAX_SOCKETS_PER_DOCUMENT
                    {
                        false
                    } else {
                        entries.insert(
                            key,
                            SocketEntry {
                                commands: sender,
                                canceled: Arc::clone(&canceled),
                                client: command.client,
                            },
                        );
                        true
                    }
                };
                if !admitted {
                    emit_failure(&sink, key);
                    return;
                }
                let registry = self.clone();
                let failed_sink = sink.clone();
                let launch = std::thread::Builder::new()
                    .name(format!("breeze-websocket-{}", command.socket_id))
                    .spawn(move || {
                        run_socket(
                            key, url, protocols, owner, client, signal, canceled, sink, receiver,
                        );
                        registry
                            .entries
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .remove(&key);
                    });
                if launch.is_err() {
                    self.entries
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .remove(&key);
                    emit_failure(&failed_sink, key);
                }
            }
            operation => {
                let mut entries = self
                    .entries
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if let Some(entry) = entries.get(&key) {
                    if entry.client != command.client || entry.commands.try_send(operation).is_err()
                    {
                        entry.canceled.store(true, Ordering::Release);
                        entries.remove(&key);
                        drop(entries);
                        emit_failure(&sink, key);
                    }
                }
            }
        }
    }
}

fn emit(sink: &WebSocketEventSink, key: (DocumentId, u64), kind: WebSocketEventKind) {
    let _ = sink.send(WebSocketEvent {
        document: key.0,
        socket_id: key.1,
        kind,
    });
}

fn emit_failure(sink: &WebSocketEventSink, key: (DocumentId, u64)) {
    emit(sink, key, WebSocketEventKind::Error);
    emit(
        sink,
        key,
        WebSocketEventKind::Close {
            code: 1006,
            reason: String::new(),
            clean: false,
        },
    );
}

pub(super) fn reject(document: DocumentId, socket_id: u64, sink: &WebSocketEventSink) {
    emit_failure(sink, (document, socket_id));
}

fn run_socket(
    key: (DocumentId, u64),
    url: String,
    protocols: Vec<String>,
    owner: RendererFetchClient,
    client: Arc<HttpClient>,
    signal: FetchSignal,
    canceled: Arc<AtomicBool>,
    sink: WebSocketEventSink,
    receiver: mpsc::Receiver<WebSocketOperation>,
) {
    if signal.is_aborted() || canceled.load(Ordering::Acquire) {
        return;
    }
    let opened = client.open_websocket(
        &url,
        &owner.url,
        &owner.origin.serialize(),
        &owner.policy,
        &protocols,
    );
    if signal.is_aborted() || canceled.load(Ordering::Acquire) {
        return;
    }
    let Ok(opened) = opened else {
        emit_failure(&sink, key);
        return;
    };
    if let Ok(WebSocketOperation::Close { code, reason }) = receiver.try_recv() {
        let _ = opened.connection.shutdown(code, &reason);
        emit_failure(&sink, key);
        return;
    }
    let socket = Arc::new(opened.connection);
    let closing = Arc::new(AtomicBool::new(false));
    let write_socket = Arc::clone(&socket);
    let write_closing = Arc::clone(&closing);
    let write_canceled = Arc::clone(&canceled);
    let write_sink = sink.clone();
    let write_signal = signal.clone();
    let writer = std::thread::Builder::new()
        .name(format!("breeze-websocket-send-{}", key.1))
        .spawn(move || {
            writer_loop(
                key,
                write_socket,
                receiver,
                write_closing,
                write_canceled,
                write_signal,
                write_sink,
            )
        });
    if writer.is_err() {
        emit_failure(&sink, key);
        return;
    }
    emit(
        &sink,
        key,
        WebSocketEventKind::Open {
            protocol: opened.protocol,
        },
    );
    loop {
        if signal.is_aborted() || canceled.load(Ordering::Acquire) {
            break;
        }
        match socket.receive() {
            Ok(WebSocketFrame::Text(text)) => emit(
                &sink,
                key,
                WebSocketEventKind::Message {
                    binary: false,
                    data: text.into_bytes(),
                },
            ),
            Ok(WebSocketFrame::Binary(data)) => emit(
                &sink,
                key,
                WebSocketEventKind::Message { binary: true, data },
            ),
            Ok(WebSocketFrame::Close { code, reason }) => {
                let already_closing = closing.swap(true, Ordering::AcqRel);
                // The peer's close frame still requires a reply. WinHTTP's
                // shutdown sends that frame unless we already sent our own.
                let replied = already_closing || socket.shutdown(code, &reason).is_ok();
                emit(
                    &sink,
                    key,
                    WebSocketEventKind::Close {
                        code,
                        reason,
                        clean: replied,
                    },
                );
                break;
            }
            Err(_) => {
                if !signal.is_aborted() && !canceled.load(Ordering::Acquire) {
                    emit_failure(&sink, key);
                }
                break;
            }
        }
    }
    canceled.store(true, Ordering::Release);
    let _ = writer.unwrap().join();
}

fn writer_loop(
    key: (DocumentId, u64),
    socket: Arc<WebSocketConnection>,
    receiver: mpsc::Receiver<WebSocketOperation>,
    closing: Arc<AtomicBool>,
    canceled: Arc<AtomicBool>,
    signal: FetchSignal,
    sink: WebSocketEventSink,
) {
    while !signal.is_aborted() {
        if canceled.load(Ordering::Acquire) {
            if !closing.load(Ordering::Acquire) {
                let _ = socket.shutdown(1001, "");
            }
            break;
        }
        let operation = match receiver.recv_timeout(Duration::from_millis(250)) {
            Ok(operation) => operation,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let sent_bytes = match &operation {
            WebSocketOperation::Send { data, .. } => Some(data.len() as u32),
            _ => None,
        };
        let result = match operation {
            WebSocketOperation::Send { binary, data } if !closing.load(Ordering::Acquire) => {
                if binary {
                    socket.send_binary(&data)
                } else {
                    std::str::from_utf8(&data)
                        .map_err(|_| "WebSocket text is not UTF-8".to_string())
                        .and_then(|text| socket.send_text(text))
                }
            }
            WebSocketOperation::Close { code, reason } => {
                closing.store(true, Ordering::Release);
                let result = socket.shutdown(code, &reason);
                if result.is_err() {
                    emit_failure(&sink, key);
                }
                break;
            }
            WebSocketOperation::Send { .. } | WebSocketOperation::Open { .. } => continue,
        };
        if result.is_err() {
            if !closing.swap(true, Ordering::AcqRel) {
                emit_failure(&sink, key);
            }
            break;
        }
        if let Some(bytes) = sent_bytes {
            // A successful native send retires these bytes from bufferedAmount.
            emit(&sink, key, WebSocketEventKind::Sent { bytes });
        }
    }
}
