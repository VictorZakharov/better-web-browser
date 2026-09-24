//! Browser-owned, origin-authoritative IndexedDB worker. The UI never does disk I/O.
use super::super::{BrowserState, tabs::TabId};
use better_web_browser::indexed_db::{
    DbOperation, DbResult, DbSession, IndexedDb, StoreDefinition, TransactionMode,
};
use better_web_browser::renderer_process::DatabaseEventSink;
use better_web_browser::renderer_protocol::{DatabaseCommand, DatabaseEvent, DocumentId};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

const QUEUE_CAPACITY: usize = 64;
const SESSION_CAPACITY: usize = 64;
const SESSION_IDLE_LIMIT: Duration = Duration::from_secs(120);

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Phase {
    Step,
    Commit,
    Abort,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum Request {
    Open {
        name: String,
    },
    List,
    Upgrade {
        name: String,
        previous_version: u64,
        version: u64,
        create: Vec<StoreDefinition>,
        remove: Vec<String>,
        writes: Vec<DbOperation>,
        #[serde(default)]
        definitions: Vec<StoreDefinition>,
    },
    Transaction {
        transaction_id: u64,
        phase: Phase,
        name: String,
        version: u64,
        mode: TransactionMode,
        operations: Vec<DbOperation>,
    },
    Delete {
        name: String,
    },
}

struct Job {
    tab_id: TabId,
    document: DocumentId,
    client_id: u64,
    request_id: u64,
    origin_url: String,
    payload: String,
    sink: DatabaseEventSink,
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct SessionKey {
    tab_id: TabId,
    document: DocumentId,
    client_id: u64,
    transaction_id: u64,
    url: String,
    name: String,
    version: u64,
}

struct SessionValue {
    session: DbSession,
    last_activity: Instant,
}

pub(in crate::windows_app) struct DatabaseWorker {
    sender: mpsc::SyncSender<Job>,
}

impl DatabaseWorker {
    pub(in crate::windows_app) fn new(database: Arc<IndexedDb>) -> Result<Self, String> {
        let (sender, receiver) = mpsc::sync_channel::<Job>(QUEUE_CAPACITY);
        std::thread::Builder::new()
            .name("breeze-indexed-db".into())
            .spawn(move || {
                let mut sessions = HashMap::new();
                while let Ok(job) = receiver.recv() {
                    sessions.retain(|_, entry: &mut SessionValue| {
                        entry.last_activity.elapsed() < SESSION_IDLE_LIMIT
                    });
                    let payload = execute(&database, &job, &mut sessions);
                    let _ = job.sink.send(DatabaseEvent {
                        document: job.document,
                        request_id: job.request_id,
                        payload: payload.to_string(),
                    });
                }
            })
            .map_err(|error| format!("start IndexedDB worker: {error}"))?;
        Ok(Self { sender })
    }

    fn submit(&self, job: Job) {
        if let Err(error) = self.sender.try_send(job) {
            let job = match error {
                mpsc::TrySendError::Full(job) | mpsc::TrySendError::Disconnected(job) => job,
            };
            let _ = job.sink.send(DatabaseEvent {
                document: job.document,
                request_id: job.request_id,
                payload: error_payload("QuotaExceededError", "IndexedDB request queue is full"),
            });
        }
    }
}

impl BrowserState {
    pub(in crate::windows_app) fn handle_database_command(
        &mut self,
        tab_id: TabId,
        command: DatabaseCommand,
    ) {
        let context = self.tabs.get_mut(tab_id).and_then(|tab| {
            tab.navigation
                .owns_document(command.document)
                .then(|| {
                    tab.renderer_session.as_ref().map(|session| {
                        (
                            tab.reader_url.clone(),
                            tab.renderer_fetches.clone(),
                            session.database_event_sink(command.document),
                        )
                    })
                })
                .flatten()
        });
        let Some((document_url, clients, sink)) = context else {
            return;
        };
        let Ok(owner) = clients.resolve_client(command.document, &document_url, command.client)
        else {
            let _ = sink.send(DatabaseEvent {
                document: command.document,
                request_id: command.request_id,
                payload: error_payload(
                    "SecurityError",
                    "IndexedDB client is not owned by this document",
                ),
            });
            return;
        };
        self.app.database_worker.submit(Job {
            tab_id,
            document: command.document,
            client_id: command.client.id,
            request_id: command.request_id,
            origin_url: owner.url,
            payload: command.payload,
            sink,
        });
    }
}

fn execute(
    database: &IndexedDb,
    job: &Job,
    sessions: &mut HashMap<SessionKey, SessionValue>,
) -> Value {
    let request: Request = match serde_json::from_str(&job.payload) {
        Ok(request) => request,
        Err(_) => {
            return json!({"kind":"error","name":"DataError","message":"Invalid IndexedDB request"});
        }
    };
    let result = match request {
        Request::Open { name } => database
            .inspect(&job.origin_url, &name)
            .map(|info| json!({"kind":"open","info":info})),
        Request::List => database
            .list(&job.origin_url)
            .map(|databases| json!({"kind":"list","databases":databases})),
        Request::Upgrade {
            name,
            previous_version,
            version,
            create,
            remove,
            writes,
            definitions,
        } => database
            .upgrade_with_schema(
                &job.origin_url,
                &name,
                previous_version,
                version,
                &create,
                &remove,
                &writes,
                &definitions,
            )
            .map(|results| results_payload("upgrade", results)),
        Request::Transaction {
            transaction_id,
            phase,
            name,
            version,
            mode,
            operations,
        } => execute_transaction(
            database,
            job,
            sessions,
            transaction_id,
            phase,
            &name,
            version,
            mode,
            &operations,
        ),
        Request::Delete { name } => database
            .delete(&job.origin_url, &name)
            .map(|old_version| json!({"kind":"delete","oldVersion":old_version})),
    };
    match result {
        Ok(value)
            if value.to_string().len() <= better_web_browser::limits::MAX_INDEXED_DB_IPC_BYTES =>
        {
            value
        }
        Ok(_) => {
            json!({"kind":"error","name":"QuotaExceededError","message":"IndexedDB result exceeds the IPC limit"})
        }
        Err(error) => json!({
            "kind":"error",
            "name":error.name(),
            "message": if matches!(error, better_web_browser::indexed_db::DbError::Persistence(_)) {
                "IndexedDB storage is unavailable".to_string()
            } else { error.to_string() },
        }),
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_transaction(
    database: &IndexedDb,
    job: &Job,
    sessions: &mut HashMap<SessionKey, SessionValue>,
    transaction_id: u64,
    phase: Phase,
    name: &str,
    version: u64,
    mode: TransactionMode,
    operations: &[DbOperation],
) -> Result<Value, better_web_browser::indexed_db::DbError> {
    use better_web_browser::indexed_db::DbError;
    if transaction_id == 0 {
        return Err(DbError::Data("invalid transaction identifier"));
    }
    let key = SessionKey {
        tab_id: job.tab_id,
        document: job.document,
        client_id: job.client_id,
        transaction_id,
        url: job.origin_url.clone(),
        name: name.into(),
        version,
    };
    match phase {
        Phase::Step => {
            if !sessions.contains_key(&key) {
                if sessions.len() >= SESSION_CAPACITY {
                    return Err(DbError::Quota);
                }
                let session = database.begin_session(&job.origin_url, name, version, mode)?;
                sessions.insert(
                    key.clone(),
                    SessionValue {
                        session,
                        last_activity: Instant::now(),
                    },
                );
            }
            let result = sessions.get_mut(&key).unwrap().session.step(operations);
            if result.is_err() {
                sessions.remove(&key);
            } else if let Some(entry) = sessions.get_mut(&key) {
                entry.last_activity = Instant::now();
            }
            result.map(|results| results_payload("transaction", results))
        }
        Phase::Commit => {
            let entry = sessions
                .remove(&key)
                .ok_or(DbError::InvalidState("transaction session expired"))?;
            database.commit_session(entry.session)?;
            Ok(results_payload("transaction", Vec::new()))
        }
        Phase::Abort => {
            sessions.remove(&key);
            Ok(results_payload("transaction", Vec::new()))
        }
    }
}

fn results_payload(kind: &str, results: Vec<DbResult>) -> Value {
    json!({"kind":kind,"results":results})
}

fn error_payload(name: &str, message: &str) -> String {
    json!({"kind":"error","name":name,"message":message}).to_string()
}
