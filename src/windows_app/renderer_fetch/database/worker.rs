//! Ordered database jobs; retirement controls bypass the bounded request budget.

use super::*;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};

const QUEUE_CAPACITY: usize = 64;

enum Work {
    Request(Job),
    RetireClient {
        tab: TabId,
        document: DocumentId,
        client: u64,
    },
    RetireDocument {
        tab: TabId,
        document: DocumentId,
    },
    RetireTab(TabId),
}

pub(in crate::windows_app) struct DatabaseWorker {
    sender: mpsc::Sender<Work>,
    pending_requests: Arc<AtomicUsize>,
}

impl DatabaseWorker {
    pub(in crate::windows_app) fn new(
        database: Arc<IndexedDb>,
        cache_storage: Arc<CacheStorage>,
    ) -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel();
        let pending_requests = Arc::new(AtomicUsize::new(0));
        let pending = Arc::clone(&pending_requests);
        std::thread::Builder::new()
            .name("breeze-origin-storage".into())
            .spawn(move || run(database, cache_storage, receiver, pending))
            .map_err(|error| format!("start origin-storage worker: {error}"))?;
        Ok(Self {
            sender,
            pending_requests,
        })
    }

    pub(super) fn submit(&self, job: Job) {
        let admitted = self
            .pending_requests
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < QUEUE_CAPACITY).then_some(count + 1)
            })
            .is_ok();
        if !admitted {
            reject(job);
        } else if let Err(mpsc::SendError(Work::Request(job))) =
            self.sender.send(Work::Request(job))
        {
            self.pending_requests.fetch_sub(1, Ordering::AcqRel);
            reject(job);
        }
    }

    pub(in crate::windows_app) fn retire_client(
        &self,
        tab: TabId,
        document: DocumentId,
        client: u64,
    ) {
        let _ = self.sender.send(Work::RetireClient {
            tab,
            document,
            client,
        });
    }

    pub(in crate::windows_app) fn retire_document(&self, tab: TabId, document: DocumentId) {
        let _ = self.sender.send(Work::RetireDocument { tab, document });
    }

    pub(in crate::windows_app) fn retire_tab(&self, tab: TabId) {
        let _ = self.sender.send(Work::RetireTab(tab));
    }
}

fn reject(job: Job) {
    let _ = job.sink.try_send(DatabaseEvent {
        document: job.document,
        request_id: job.request_id,
        payload: error_payload("QuotaExceededError", "Origin-storage request queue is full"),
    });
}

fn run(
    database: Arc<IndexedDb>,
    cache_storage: Arc<CacheStorage>,
    receiver: mpsc::Receiver<Work>,
    pending: Arc<AtomicUsize>,
) {
    let mut sessions: HashMap<SessionKey, SessionValue> = HashMap::new();
    // A terminated Worker's committed client identity cannot be reused within
    // the document. Remember it until document retirement so a late Step cannot
    // create a new session after the ordered abort control.
    let mut retired_clients: HashSet<(TabId, DocumentId, u64)> = HashSet::new();
    let mut failed_documents: HashSet<(TabId, DocumentId)> = HashSet::new();
    loop {
        // A quiet browser still expires abandoned sessions; no subsequent DB job is needed.
        let incoming = if let Some(wait) = sessions
            .values()
            .map(|entry| SESSION_IDLE_LIMIT.saturating_sub(entry.last_activity.elapsed()))
            .min()
        {
            receiver.recv_timeout(wait)
        } else {
            receiver.recv().map_err(|_| RecvTimeoutError::Disconnected)
        };
        sessions.retain(|_, entry| entry.last_activity.elapsed() < SESSION_IDLE_LIMIT);
        let work = match incoming {
            Ok(work) => work,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        match work {
            Work::Request(job) => {
                pending.fetch_sub(1, Ordering::AcqRel);
                let payload = if failed_documents.contains(&(job.tab_id, job.document)) {
                    json!({"kind":"error","name":"AbortError","message":"Origin-storage document is unavailable"})
                } else if retired_clients.contains(&(job.tab_id, job.document, job.client_id)) {
                    json!({"kind":"error","name":"AbortError","message":"Origin-storage client has been retired"})
                } else {
                    execute(&database, &cache_storage, &job, &mut sessions)
                };
                if job
                    .sink
                    .try_send(DatabaseEvent {
                        document: job.document,
                        request_id: job.request_id,
                        payload: payload.to_string(),
                    })
                    .is_err()
                {
                    // The broker fails an overfull renderer explicitly. Discard
                    // staged writes and prevent queued jobs from reviving them.
                    fail_document(
                        &mut sessions,
                        &mut failed_documents,
                        job.tab_id,
                        job.document,
                    );
                }
            }
            other => retire(
                &mut sessions,
                &mut retired_clients,
                &mut failed_documents,
                other,
            ),
        }
    }
}

fn fail_document(
    sessions: &mut HashMap<SessionKey, SessionValue>,
    failed_documents: &mut HashSet<(TabId, DocumentId)>,
    tab: TabId,
    document: DocumentId,
) {
    failed_documents.insert((tab, document));
    sessions.retain(|key, _| key.tab_id != tab || key.document != document);
}

fn retire(
    sessions: &mut HashMap<SessionKey, SessionValue>,
    retired_clients: &mut HashSet<(TabId, DocumentId, u64)>,
    failed_documents: &mut HashSet<(TabId, DocumentId)>,
    work: Work,
) {
    match work {
        Work::RetireClient {
            tab,
            document,
            client,
        } => {
            retired_clients.insert((tab, document, client));
            sessions.retain(|key, _| {
                !(key.tab_id == tab && key.document == document && key.client_id == client)
            });
        }
        Work::RetireDocument { tab, document } => {
            failed_documents.remove(&(tab, document));
            retired_clients
                .retain(|&(owner, generation, _)| owner != tab || generation != document);
            sessions.retain(|key, _| !(key.tab_id == tab && key.document == document));
        }
        Work::RetireTab(tab) => {
            failed_documents.retain(|&(owner, _)| owner != tab);
            retired_clients.retain(|&(owner, _, _)| owner != tab);
            sessions.retain(|key, _| key.tab_id != tab);
        }
        Work::Request(_) => unreachable!("requests execute in the worker loop"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use better_web_browser::indexed_db::{DbOperation, Key};

    #[test]
    fn terminating_worker_aborts_staged_write_and_rejects_late_steps() {
        let database = IndexedDb::in_memory();
        let origin = "https://example.test";
        database
            .upgrade(
                origin,
                "worker-retirement",
                0,
                1,
                &[StoreDefinition {
                    name: "items".into(),
                    key_path: None,
                    auto_increment: false,
                    indexes: Vec::new(),
                }],
                &[],
                &[],
            )
            .unwrap();
        let tab = TabId::first();
        let document = DocumentId::new(1).unwrap();
        let client = (1_u64 << 63) | 1;
        let mut session = database
            .begin_session(origin, "worker-retirement", 1, TransactionMode::ReadWrite)
            .unwrap();
        session
            .step(&[DbOperation::Put {
                store: "items".into(),
                key: Some(Key::String("uncommitted".into())),
                value: "staged".into(),
                overwrite: true,
            }])
            .unwrap();
        let key = SessionKey {
            tab_id: tab,
            document,
            client_id: client,
            transaction_id: 3,
            url: origin.into(),
            name: "worker-retirement".into(),
            version: 1,
        };
        let mut sessions = HashMap::from([(
            key,
            SessionValue {
                session,
                last_activity: Instant::now(),
            },
        )]);
        let mut retired_clients = HashSet::new();
        let mut failed_documents = HashSet::new();
        retire(
            &mut sessions,
            &mut retired_clients,
            &mut failed_documents,
            Work::RetireClient {
                tab,
                document,
                client,
            },
        );
        assert!(
            sessions.is_empty(),
            "termination must free the live session slot"
        );
        assert!(retired_clients.contains(&(tab, document, client)));
        assert!(
            matches!(
                database
                    .transaction(
                        origin,
                        "worker-retirement",
                        1,
                        TransactionMode::ReadOnly,
                        &[DbOperation::Get {
                            store: "items".into(),
                            key: Key::String("uncommitted".into()),
                        }]
                    )
                    .unwrap()
                    .as_slice(),
                [DbResult::Value(None)]
            ),
            "termination must not persist a staged write"
        );
        retire(
            &mut sessions,
            &mut retired_clients,
            &mut failed_documents,
            Work::RetireDocument { tab, document },
        );
        assert!(
            retired_clients.is_empty(),
            "retired clients have document lifetime"
        );

        let session = database
            .begin_session(origin, "worker-retirement", 1, TransactionMode::ReadWrite)
            .unwrap();
        sessions.insert(
            SessionKey {
                tab_id: tab,
                document,
                client_id: client,
                transaction_id: 4,
                url: origin.into(),
                name: "worker-retirement".into(),
                version: 1,
            },
            SessionValue {
                session,
                last_activity: Instant::now(),
            },
        );
        fail_document(&mut sessions, &mut failed_documents, tab, document);
        assert!(
            sessions.is_empty(),
            "lost required reply must free document sessions"
        );
        assert!(failed_documents.contains(&(tab, document)));
        retire(
            &mut sessions,
            &mut retired_clients,
            &mut failed_documents,
            Work::RetireDocument { tab, document },
        );
        assert!(failed_documents.is_empty());
    }
}
