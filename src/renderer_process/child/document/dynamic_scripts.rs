//! Nonblocking Fetch ownership for dynamically prepared external classic scripts.

use super::fetch::page_resource_request;
use super::resources::decode_script_response;
use super::*;
use crate::engine::dom::NodeId;
use crate::renderer_process::child::connection::PendingFetchBatch;

struct Owners {
    resource: PageResource,
    nodes: Vec<NodeId>,
}

pub(super) struct PendingDynamicScriptFetch {
    batch: PendingFetchBatch,
    by_request: HashMap<u64, Owners>,
}

impl DocumentRuntime {
    pub(super) fn start_dynamic_script_fetches(
        &mut self,
        connection: &mut ChildConnection,
    ) -> Result<(), String> {
        let Some(runtime) = self.script_runtime.as_mut() else {
            return Ok(());
        };
        let mut requests = Vec::new();
        let mut by_request: HashMap<u64, Owners> = HashMap::new();
        for script in runtime.take_dynamic_script_requests() {
            let resource = PageResource::Script {
                url: script.source_url,
                kind: script.kind,
                fetch_options: script.fetch_options,
            };
            // Share in-flight bytes without conflating each element's execution or events.
            let owners = self
                .pending_dynamic_script_fetch
                .iter_mut()
                .flat_map(|pending| pending.by_request.values_mut())
                .chain(by_request.values_mut())
                .find(|owners| owners.resource == resource);
            if let Some(owners) = owners {
                owners.nodes.push(script.node);
                continue;
            }
            let id = connection.allocate_request_id();
            requests.push(page_resource_request(id, self.id, &resource));
            by_request.insert(
                id,
                Owners {
                    resource,
                    nodes: vec![script.node],
                },
            );
        }
        if let Some(batch) = connection.start_fetch_batch(self.id, requests)? {
            self.pending_dynamic_script_fetch
                .push(PendingDynamicScriptFetch { batch, by_request });
        }
        Ok(())
    }

    pub(super) fn finish_ready_dynamic_scripts(
        &mut self,
        connection: &mut ChildConnection,
    ) -> Result<(), String> {
        for mut pending in std::mem::take(&mut self.pending_dynamic_script_fetch) {
            for response in connection.take_ready_fetch_batch(&mut pending.batch)? {
                let owners = pending
                    .by_request
                    .remove(&response.head.request_id)
                    .ok_or_else(|| "dynamic script response has no prepared owner".to_string())?;
                let result = decode_script_response(response, ScriptKind::Classic);
                if let Some(runtime) = self.script_runtime.as_mut() {
                    for node in owners.nodes {
                        runtime.complete_dynamic_script(node, result.clone());
                    }
                }
            }
            if !pending.batch.is_empty() {
                self.pending_dynamic_script_fetch.push(pending);
            }
        }
        Ok(())
    }
}

pub(super) fn advance_dynamic_script_slice(
    runtime: &mut ScriptRuntime,
    document_root: NodeId,
    elapsed: Duration,
    max_tasks: usize,
) -> ScriptOutcome {
    let started = Instant::now();
    let mut aggregate = ScriptOutcome::default();
    let mut advance = elapsed;
    for _ in 0..max_tasks {
        let outcome = runtime.advance_time(advance, 1);
        advance = Duration::ZERO;
        let stopped = outcome.runtime_stopped || outcome.navigation_url.is_some();
        merge_outcome(&mut aggregate, outcome, document_root);
        if stopped
            || !runtime.has_ready_dynamic_scripts()
            || started.elapsed() >= Duration::from_millis(25)
        {
            break;
        }
    }
    aggregate
}
