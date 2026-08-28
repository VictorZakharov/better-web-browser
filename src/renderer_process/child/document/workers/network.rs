//! Non-blocking bridge between Worker threads and the browser network process.

use super::super::fetch::{into_fetch_result, script_api_request};
use super::*;
use crate::fetch::{FetchError, FetchErrorKind, FetchRequest, FetchResponse, RequestMode};
use crate::limits::MAX_RENDERER_FETCH_REQUESTS_PER_BATCH;
use crate::renderer_process::child::connection::PendingFetchBatch;

pub(super) struct WorkerNetworkRequest {
    pub(super) request: FetchRequest,
    reply: mpsc::Sender<Result<FetchResponse, FetchError>>,
}

pub(super) struct PendingWorkerFetch {
    batch: PendingFetchBatch,
    replies: HashMap<u64, mpsc::Sender<Result<FetchResponse, FetchError>>>,
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
        .collect::<Vec<_>>();
    if requests.is_empty() {
        return Ok(());
    }
    let mut replies = HashMap::with_capacity(requests.len());
    let wire = requests
        .into_iter()
        .map(|request| {
            let request_id = connection.allocate_request_id();
            replies.insert(request_id, request.reply);
            script_api_request(request_id, document, request.request)
        })
        .collect::<Vec<_>>();
    let Some(batch) = connection.start_fetch_batch(document, wire)? else {
        return Ok(());
    };
    // Worker network traffic is intentionally not finished here. Waiting for a response while
    // handling AdvanceClock turns a slow origin into a page-renderer watchdog failure.
    pending.push(PendingWorkerFetch { batch, replies });
    Ok(())
}

pub(super) fn finish_ready_network_batches(
    connection: &mut ChildConnection,
    pending: &mut Vec<PendingWorkerFetch>,
) -> Result<(), String> {
    let mut index = 0;
    while index < pending.len() {
        let responses = connection.take_ready_fetch_batch(&mut pending[index].batch)?;
        for response in responses {
            let request_id = response.head.request_id;
            let reply = pending[index]
                .replies
                .remove(&request_id)
                .ok_or_else(|| "browser returned an unknown Worker Fetch response".to_string())?;
            let _ = reply.send(into_fetch_result(response));
        }
        if pending[index].batch.is_empty() {
            pending.swap_remove(index);
        } else {
            index += 1;
        }
    }
    Ok(())
}

pub(super) fn worker_source_request(
    network: &mpsc::Sender<WorkerNetworkRequest>,
    document_url: &str,
    url: &str,
    kind: ScriptKind,
    credentials: CredentialsMode,
) -> Result<FetchResponse, FetchError> {
    let mut request = FetchRequest::script(url, document_url)?;
    request.mode = match kind {
        ScriptKind::Classic => RequestMode::SameOrigin,
        ScriptKind::Module => RequestMode::Cors,
    };
    request.credentials = credentials;
    request_network(network, request)
}

pub(super) fn request_network(
    network: &mpsc::Sender<WorkerNetworkRequest>,
    request: FetchRequest,
) -> Result<FetchResponse, FetchError> {
    let (reply, response) = mpsc::channel();
    network
        .send(WorkerNetworkRequest { request, reply })
        .map_err(|_| FetchError::new(FetchErrorKind::Network, "Worker broker disconnected"))?;
    response
        .recv()
        .map_err(|_| FetchError::new(FetchErrorKind::Network, "Worker broker stopped"))?
}
