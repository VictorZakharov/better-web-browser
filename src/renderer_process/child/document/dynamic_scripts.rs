//! Concurrent fetch ownership for scripts inserted by the retained document runtime.

use super::fetch::page_resource_request;
use super::reporting::merge_outcome;
use super::resources::{decode_script_response, fetch_script_source};
use crate::engine::{
    DynamicScriptRequest, PageResource, ScriptFetchOptions, ScriptKind, ScriptOutcome,
    ScriptRuntime,
};
use crate::renderer_process::child::connection::{ChildConnection, PendingFetchBatch};
use crate::renderer_protocol::DocumentId;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

const DYNAMIC_TASK_WALL_SLICE: Duration = Duration::from_millis(25);

pub(super) struct PendingDynamicScriptFetch {
    batch: PendingFetchBatch,
    by_request: HashMap<u64, DynamicScriptRequest>,
}

pub(super) fn start_dynamic_script_preloads(
    connection: &mut ChildConnection,
    document: DocumentId,
    scripts: Vec<DynamicScriptRequest>,
) -> Result<Option<PendingDynamicScriptFetch>, String> {
    let mut by_request = HashMap::with_capacity(scripts.len());
    let requests = scripts
        .into_iter()
        .map(|script| {
            let request_id = connection.allocate_request_id();
            let resource = PageResource::Script {
                url: script.source_url.clone(),
                kind: script.kind,
                fetch_options: script.fetch_options,
            };
            by_request.insert(request_id, script);
            page_resource_request(request_id, document, &resource)
        })
        .collect();
    let Some(batch) = connection.start_fetch_batch(document, requests)? else {
        return Ok(None);
    };
    Ok(Some(PendingDynamicScriptFetch { batch, by_request }))
}

pub(super) fn finish_dynamic_script_source(
    pending: &mut Option<PendingDynamicScriptFetch>,
    connection: &mut ChildConnection,
    document: DocumentId,
    url: &str,
    kind: ScriptKind,
    fetch_options: ScriptFetchOptions,
) -> Result<String, String> {
    let Some(mut active) = pending.take() else {
        return fetch_script_source(connection, document, url, kind, fetch_options);
    };
    let request_id = active.by_request.iter().find_map(|(request_id, script)| {
        (script.source_url == url && script.kind == kind && script.fetch_options == fetch_options)
            .then_some(*request_id)
    });
    let Some(request_id) = request_id else {
        *pending = Some(active);
        return fetch_script_source(connection, document, url, kind, fetch_options);
    };

    active.by_request.remove(&request_id);
    let (selected, remaining) = active.batch.split(HashSet::from([request_id]))?;
    if let Some(batch) = remaining {
        *pending = Some(PendingDynamicScriptFetch {
            batch,
            by_request: active.by_request,
        });
    }
    let response = connection
        .finish_fetch_batch(selected.expect("a selected dynamic script request exists"))?
        .pop()
        .ok_or_else(|| "browser omitted a dynamic script response".to_string())?;
    decode_script_response(response, kind)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn advance_dynamic_script_slice(
    runtime: &mut ScriptRuntime,
    pending: &mut Option<PendingDynamicScriptFetch>,
    connection: &mut ChildConnection,
    document: DocumentId,
    document_root: crate::engine::dom::NodeId,
    elapsed: Duration,
    max_tasks: usize,
    script_fetch_time: &mut Duration,
) -> ScriptOutcome {
    let slice_started = Instant::now();
    let mut aggregate = ScriptOutcome::default();
    let mut task_advance = elapsed;
    for _ in 0..max_tasks {
        if !runtime.has_pending_dynamic_scripts() {
            break;
        }
        let mut loader = |url: &str, kind, options| {
            let started = Instant::now();
            let result =
                finish_dynamic_script_source(pending, connection, document, url, kind, options);
            *script_fetch_time += started.elapsed();
            result
        };
        let outcome = runtime.advance_time_with_loader(task_advance, 1, Some(&mut loader));
        task_advance = Duration::ZERO;
        let should_stop = outcome.runtime_stopped || outcome.navigation_url.is_some();
        merge_outcome(&mut aggregate, outcome, document_root);
        if should_stop || slice_started.elapsed() >= DYNAMIC_TASK_WALL_SLICE {
            break;
        }
    }
    aggregate
}
