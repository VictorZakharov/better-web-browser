//! Non-blocking bridge between Worker threads and the browser network process.

use super::super::fetch::{into_fetch_result, script_api_request};
use super::*;
use crate::fetch::{
    FetchError, FetchErrorKind, FetchRequest, FetchResponse, RequestContext, RequestDestination,
    RequestMode,
};
use crate::limits::MAX_RENDERER_FETCH_REQUESTS_PER_BATCH;
use crate::renderer_process::child::connection::PendingFetchBatch;

pub(super) struct WorkerNetworkRequest {
    pub(super) request: FetchRequest,
    reply: mpsc::Sender<Result<FetchResponse, FetchError>>,
    cancelled: Arc<AtomicBool>,
}

struct PendingReply {
    sender: mpsc::Sender<Result<FetchResponse, FetchError>>,
    cancelled: Arc<AtomicBool>,
    aborted: bool,
}

pub(super) struct PendingWorkerFetch {
    batch: PendingFetchBatch,
    document: DocumentId,
    replies: HashMap<u64, PendingReply>,
}

pub(super) fn start_ready_network_batch(
    connection: &mut ChildConnection,
    document: DocumentId,
    network: &mpsc::Receiver<WorkerNetworkRequest>,
    pending: &mut Vec<PendingWorkerFetch>,
) -> Result<(), String> {
    let requests = network
        .try_iter()
        .take(MAX_RENDERER_FETCH_REQUESTS_PER_BATCH)
        .filter(|request| !request.cancelled.load(Ordering::Acquire))
        .collect::<Vec<_>>();
    if requests.is_empty() {
        return Ok(());
    }
    let mut replies = HashMap::with_capacity(requests.len());
    let wire = requests
        .into_iter()
        .map(|request| {
            let request_id = connection.allocate_request_id();
            replies.insert(
                request_id,
                PendingReply {
                    sender: request.reply,
                    cancelled: request.cancelled,
                    aborted: false,
                },
            );
            script_api_request(request_id, document, request.request)
        })
        .collect::<Vec<_>>();
    let Some(batch) = connection.start_fetch_batch(document, wire)? else {
        return Ok(());
    };
    // Worker network traffic is intentionally not finished here. Waiting for a response while
    // handling AdvanceClock turns a slow origin into a page-renderer watchdog failure.
    pending.push(PendingWorkerFetch {
        batch,
        document,
        replies,
    });
    Ok(())
}

pub(super) fn finish_ready_network_batches(
    connection: &mut ChildConnection,
    pending: &mut Vec<PendingWorkerFetch>,
) -> Result<(), String> {
    let mut index = 0;
    while index < pending.len() {
        let current = &mut pending[index];
        for (id, reply) in &mut current.replies {
            if !reply.aborted && reply.cancelled.load(Ordering::Acquire) {
                connection.abort_fetch(current.document, *id)?;
                reply.aborted = true;
            }
        }
        let responses = connection.take_ready_fetch_batch(&mut pending[index].batch)?;
        for response in responses {
            let request_id = response.head.request_id;
            let reply = pending[index]
                .replies
                .remove(&request_id)
                .ok_or_else(|| "browser returned an unknown Worker Fetch response".to_string())?;
            let _ = reply.sender.send(into_fetch_result(response));
        }
        if pending[index].batch.is_empty() {
            pending.swap_remove(index);
        } else {
            index += 1;
        }
    }
    Ok(())
}

pub(super) struct WorkerSourceRequest {
    pub(super) network: mpsc::Sender<WorkerNetworkRequest>,
    pub(super) cancelled: Arc<AtomicBool>,
    pub(super) base_url: String,
    pub(super) credentials: CredentialsMode,
    pub(super) creator_client: crate::fetch::RequestClient,
    pub(super) worker_client: crate::fetch::RequestClient,
}

pub(super) fn worker_source_request(
    source: &WorkerSourceRequest,
    url: &str,
    kind: ScriptKind,
    entry: bool,
) -> Result<FetchResponse, FetchError> {
    let mut request = FetchRequest::script(url, &source.base_url)?;
    request.context = RequestContext::WorkerScript;
    if entry {
        request.destination = RequestDestination::Worker;
        request.resulting_client = source.worker_client;
    } else {
        request.destination = RequestDestination::Script;
        // HTML assigns `not parser-inserted` metadata to worker-imported scripts.
        request.script_source = Some(crate::fetch::csp::ScriptSource {
            nonce: None,
            parser_inserted: false,
        });
    }
    request.mode = match kind {
        ScriptKind::Classic if entry => RequestMode::SameOrigin,
        // HTML's classic importScripts fetch is no-cors, unlike the
        // same-origin top-level classic Worker script fetch.
        ScriptKind::Classic => RequestMode::NoCors,
        ScriptKind::Module => RequestMode::Cors,
    };
    request.credentials = source.credentials;
    request.client = if entry {
        source.creator_client
    } else {
        source.worker_client
    };
    request_network(&source.network, &source.cancelled, request)
}

pub(super) fn request_network(
    network: &mpsc::Sender<WorkerNetworkRequest>,
    cancelled: &Arc<AtomicBool>,
    request: FetchRequest,
) -> Result<FetchResponse, FetchError> {
    let (reply, response) = mpsc::channel();
    network
        .send(WorkerNetworkRequest {
            request,
            reply,
            cancelled: cancelled.clone(),
        })
        .map_err(|_| FetchError::new(FetchErrorKind::Network, "Worker broker disconnected"))?;
    // importScripts is synchronous, but termination must not wait for its server.
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err(FetchError::new(
                FetchErrorKind::Network,
                "Worker terminated",
            ));
        }
        match response.recv_timeout(Duration::from_millis(25)) {
            Ok(response) => return response,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(FetchError::new(
                    FetchErrorKind::Network,
                    "Worker broker stopped",
                ));
            }
        }
    }
}
