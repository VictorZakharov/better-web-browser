//! Browser-wide socket-pair budget and document/client ownership for sockets.

use super::*;
use better_web_browser::fetch::RequestClient;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;

const MAX_SOCKETS_PER_DOCUMENT: usize = 8;
// A connected socket owns a receiver and a writer thread. Reserve before the
// handshake and release only after the writer has joined, bounding both.
const MAX_NATIVE_SOCKET_PAIRS: usize = 32;
const COMMAND_QUEUE: usize = 32;
static NATIVE_SOCKET_PAIRS: AtomicUsize = AtomicUsize::new(0);

struct SocketPairPermit;

struct SocketThreadGuard {
    registry: RendererWebSocketRegistry,
    key: (DocumentId, u64),
    identity: Arc<AtomicBool>,
    _permit: SocketPairPermit,
}

impl Drop for SocketThreadGuard {
    fn drop(&mut self) {
        self.registry.retire_if_current(self.key, &self.identity);
    }
}

impl SocketPairPermit {
    fn reserve() -> Option<Self> {
        NATIVE_SOCKET_PAIRS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_NATIVE_SOCKET_PAIRS).then_some(count + 1)
            })
            .ok()
            .map(|_| Self)
    }
}

impl Drop for SocketPairPermit {
    fn drop(&mut self) {
        NATIVE_SOCKET_PAIRS.fetch_sub(1, Ordering::AcqRel);
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
        let entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for entry in entries.values() {
            cancel_entry(entry);
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
            WebSocketOperation::Cancel => self.cancel_socket(key, command.client),
            WebSocketOperation::Open { url, protocols } => {
                let (sender, receiver) = mpsc::sync_channel(COMMAND_QUEUE);
                let canceled = Arc::new(AtomicBool::new(false));
                let (permit, notify_failure) = {
                    let mut entries = self
                        .entries
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    if let Some(existing) = entries.get(&key) {
                        (None, existing.client == command.client)
                    } else if entries
                        .keys()
                        .filter(|(document, _)| *document == command.document)
                        .count()
                        >= MAX_SOCKETS_PER_DOCUMENT
                    {
                        (None, true)
                    } else {
                        let permit = SocketPairPermit::reserve();
                        if permit.is_some() {
                            entries.insert(
                                key,
                                SocketEntry {
                                    commands: sender,
                                    canceled: Arc::clone(&canceled),
                                    client: command.client,
                                },
                            );
                        }
                        (permit, true)
                    }
                };
                let Some(permit) = permit else {
                    if notify_failure {
                        emit_failure(&sink, key);
                    }
                    return;
                };
                let registry = self.clone();
                let identity = Arc::clone(&canceled);
                let cleanup_identity = Arc::clone(&canceled);
                let failed_sink = sink.clone();
                let launch = std::thread::Builder::new()
                    .name(format!("breeze-websocket-{}", command.socket_id))
                    .spawn(move || {
                        let _guard = SocketThreadGuard {
                            registry,
                            key,
                            identity,
                            _permit: permit,
                        };
                        run_socket(SocketJob {
                            key,
                            url,
                            protocols,
                            owner,
                            client,
                            signal,
                            canceled,
                            sink,
                            receiver,
                        });
                    });
                if launch.is_err() {
                    // Failed spawn drops its captured permit; no native thread exists.
                    self.retire_if_current(key, &cleanup_identity);
                    emit_failure(&failed_sink, key);
                }
            }
            operation => {
                if self.enqueue_existing(key, command.client, operation) {
                    emit_failure(&sink, key);
                }
            }
        }
    }

    fn cancel_socket(&self, key: (DocumentId, u64), client: RequestClient) {
        let entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(entry) = entries.get(&key)
            && entry.client == client
        {
            cancel_entry(entry);
        }
    }

    // Only a saturated/disconnected owner queue emits failure. Another client's
    // command is ignored, without changing the real owner's connection.
    fn enqueue_existing(
        &self,
        key: (DocumentId, u64),
        client: RequestClient,
        operation: WebSocketOperation,
    ) -> bool {
        let entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(entry) = entries.get(&key) else {
            return false;
        };
        if entry.client != client || entry.canceled.load(Ordering::Acquire) {
            return false;
        }
        if entry.commands.try_send(operation).is_err() {
            cancel_entry(entry);
            return true;
        }
        false
    }

    fn retire_if_current(&self, key: (DocumentId, u64), identity: &Arc<AtomicBool>) {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if entries
            .get(&key)
            .is_some_and(|entry| Arc::ptr_eq(&entry.canceled, identity))
        {
            entries.remove(&key);
        }
    }
}

fn cancel_entry(entry: &SocketEntry) {
    if entry.canceled.swap(true, Ordering::AcqRel) {
        return;
    }
    let _ = entry.commands.try_send(WebSocketOperation::Close {
        code: 1001,
        reason: String::new(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foreign_client_cannot_close_or_cancel_another_clients_socket() {
        let document = DocumentId::new(1).unwrap();
        let key = (document, 7);
        let owner = RequestClient {
            id: 5,
            opaque: false,
        };
        let foreign = RequestClient {
            id: 6,
            opaque: false,
        };
        let (sender, receiver) = mpsc::sync_channel(2);
        let canceled = Arc::new(AtomicBool::new(false));
        let registry = RendererWebSocketRegistry::default();
        registry.entries.lock().unwrap().insert(
            key,
            SocketEntry {
                commands: sender,
                canceled: Arc::clone(&canceled),
                client: owner,
            },
        );
        assert!(!registry.enqueue_existing(
            key,
            foreign,
            WebSocketOperation::Close {
                code: 1000,
                reason: String::new()
            }
        ));
        registry.cancel_socket(key, foreign);
        assert!(!canceled.load(Ordering::Acquire));
        assert!(receiver.try_recv().is_err());
        assert!(!registry.enqueue_existing(
            key,
            owner,
            WebSocketOperation::Send {
                binary: false,
                data: b"owned".to_vec()
            }
        ));
        assert!(
            matches!(receiver.try_recv(), Ok(WebSocketOperation::Send { data, .. })
            if data == b"owned")
        );
    }

    #[test]
    fn canceled_entry_stays_counted_and_stale_cleanup_cannot_remove_replacement() {
        let document = DocumentId::new(1).unwrap();
        let key = (document, 7);
        let (old_sender, _) = mpsc::sync_channel(1);
        let old = Arc::new(AtomicBool::new(false));
        let registry = RendererWebSocketRegistry::default();
        registry.entries.lock().unwrap().insert(
            key,
            SocketEntry {
                commands: old_sender,
                canceled: Arc::clone(&old),
                client: RequestClient::default(),
            },
        );
        registry.cancel_all();
        assert!(old.load(Ordering::Acquire));
        assert_eq!(registry.entries.lock().unwrap().len(), 1);
        let (new_sender, _) = mpsc::sync_channel(1);
        let new = Arc::new(AtomicBool::new(false));
        registry.entries.lock().unwrap().insert(
            key,
            SocketEntry {
                commands: new_sender,
                canceled: Arc::clone(&new),
                client: RequestClient::default(),
            },
        );
        registry.retire_if_current(key, &old);
        assert!(Arc::ptr_eq(
            &registry.entries.lock().unwrap().get(&key).unwrap().canceled,
            &new
        ));
        registry.retire_if_current(key, &new);
        assert!(registry.entries.lock().unwrap().is_empty());
    }
}
