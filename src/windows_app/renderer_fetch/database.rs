//! Browser-owned, origin-authoritative IndexedDB worker. The UI never does disk I/O.
mod worker;
pub(in crate::windows_app) use worker::DatabaseWorker;

use super::super::{BrowserState, tabs::TabId};
use better_web_browser::cache_storage::{CacheCommand, CacheStorage};
use better_web_browser::indexed_db::{
    DbOperation, DbResult, DbSession, IndexedDb, StoreDefinition, TransactionMode,
};
use better_web_browser::renderer_process::DatabaseEventSink;
use better_web_browser::renderer_protocol::{
    DATABASE_RETIRE_CLIENT_PAYLOAD, DatabaseCommand, DatabaseEvent, DocumentId,
};
use better_web_browser::storage::LocalStorage;
use better_web_browser::storage_manager::StorageEstimate;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
    StorageEstimate,
    Cache {
        command: CacheCommand,
    },
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
            let _ = sink.try_send(DatabaseEvent {
                document: command.document,
                request_id: command.request_id,
                payload: error_payload(
                    "SecurityError",
                    "Storage client is not owned by this document",
                ),
            });
            return;
        };
        if command.payload == DATABASE_RETIRE_CLIENT_PAYLOAD {
            // Only a committed Worker client may retire its own sessions. The
            // control remains ordered after its earlier Step requests.
            if command.client.id & (1_u64 << 63) != 0 && command.request_id & (1_u64 << 63) != 0 {
                self.app
                    .database_worker
                    .retire_client(tab_id, command.document, command.client.id);
            }
            return;
        }
        // The committed client origin is authoritative. Its URL can remain a
        // non-opaque response URL even when sandboxing narrows the client origin.
        let Some(origin) = database_origin(&owner) else {
            let _ = sink.try_send(DatabaseEvent {
                document: command.document,
                request_id: command.request_id,
                payload: error_payload(
                    "SecurityError",
                    "Storage is unavailable for an opaque origin",
                ),
            });
            return;
        };
        self.app.database_worker.submit(Job {
            tab_id,
            document: command.document,
            client_id: command.client.id,
            request_id: command.request_id,
            origin_url: origin,
            payload: command.payload,
            sink,
        });
    }
}

impl BrowserState {
    pub(in crate::windows_app) fn retire_database_for_tab(&self, tab: TabId) {
        if let Some(document) = self
            .tabs
            .iter()
            .find(|candidate| candidate.id == tab)
            .and_then(|candidate| candidate.navigation.active_document())
        {
            self.app.database_worker.retire_document(tab, document);
        }
    }
}

fn database_origin(client: &super::clients::Client) -> Option<String> {
    let origin = client.origin.serialize();
    (origin != "null").then_some(origin)
}

fn execute(
    database: &IndexedDb,
    cache_storage: &CacheStorage,
    local_storage: &LocalStorage,
    job: &Job,
    sessions: &mut HashMap<SessionKey, SessionValue>,
) -> Value {
    let request: Request = match serde_json::from_str(&job.payload) {
        Ok(request) => request,
        Err(_) => {
            return json!({"kind":"error","name":"DataError","message":"Invalid storage request"});
        }
    };
    if matches!(request, Request::StorageEstimate) {
        return match StorageEstimate::for_origin(
            &job.origin_url,
            local_storage,
            database,
            cache_storage,
        ) {
            Ok(estimate) => json!({"kind":"storageEstimate","value":estimate}),
            Err(_) => {
                json!({"kind":"error","name":"UnknownError", "message":"Storage estimate is unavailable"})
            }
        };
    }
    if let Request::Cache { command } = request {
        let result = cache_storage.execute(&job.origin_url, command);
        return match result {
            Ok(value) => {
                let reply = json!({"kind":"cache","value":value});
                if reply.to_string().len() <= better_web_browser::limits::MAX_INDEXED_DB_IPC_BYTES {
                    reply
                } else {
                    json!({"kind":"error","name":"QuotaExceededError","message":"CacheStorage result exceeds the IPC limit"})
                }
            }
            Err(error) => json!({
                "kind":"error",
                "name":error.name(),
                "message": if matches!(error, better_web_browser::cache_storage::CacheError::Persistence(_)) {
                    "CacheStorage is unavailable".to_string()
                } else { error.to_string() },
            }),
        };
    }
    let result = match request {
        Request::StorageEstimate => unreachable!("handled above"),
        Request::Cache { .. } => unreachable!("handled above"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use better_web_browser::fetch::Origin;

    #[test]
    fn database_origin_uses_the_committed_client_not_its_url() {
        let mut client = super::super::clients::Client {
            policy: Arc::new(Default::default()),
            url: "https://host.test/worker.js".into(),
            origin: Origin::opaque(),
        };
        assert_eq!(database_origin(&client), None);
        client.origin = Origin::parse("https://other.test/page").unwrap();
        assert_eq!(
            database_origin(&client).as_deref(),
            Some("https://other.test")
        );
    }
}
