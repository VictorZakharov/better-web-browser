//! Event-loop-friendly script Fetch lifecycle and response-stream delivery.

use super::super::fetch::{into_fetch_error, into_fetch_head_result, script_api_request};
use super::super::reporting::{micros, runtime_report};
use super::super::{AdvanceResult, DocumentRuntime};
use crate::engine::{ScriptFetchAction, ScriptFetchEvent, ScriptOutcome, StyleRefreshStats};
use crate::renderer_process::child::connection::{ChildConnection, ScriptFetchDelivery};
use crate::renderer_protocol::{PageLoadReport, RendererRuntimeUpdate};
use std::collections::HashSet;
use std::time::Instant;

impl DocumentRuntime {
    pub(in crate::renderer_process::child) fn start_pending_fetches(
        &mut self,
        connection: &mut ChildConnection,
    ) -> Result<(), String> {
        let actions = std::mem::take(&mut self.pending_fetches);
        let aborted = actions
            .iter()
            .filter_map(|action| match action {
                ScriptFetchAction::Abort { id } => Some(*id),
                ScriptFetchAction::Start { .. } => None,
            })
            .collect::<HashSet<_>>();

        let active_to_abort = self
            .active_script_fetches
            .iter()
            .filter_map(|(wire_id, script_id)| aborted.contains(script_id).then_some(*wire_id))
            .collect::<Vec<_>>();
        for wire_id in active_to_abort {
            connection.abort_fetch(self.id, wire_id)?;
            self.active_script_fetches.remove(&wire_id);
        }

        let mut requests = Vec::new();
        let mut started = Vec::new();
        for action in actions {
            if let ScriptFetchAction::Start { id, request } = action
                && !aborted.contains(&id)
            {
                let wire_id = connection.allocate_request_id();
                requests.push(script_api_request(wire_id, self.id, *request));
                started.push((wire_id, id));
            }
        }
        if requests.is_empty() {
            return Ok(());
        }
        connection.start_streaming_fetch_batch(self.id, requests)?;
        self.active_script_fetches.extend(started);
        Ok(())
    }

    pub(in crate::renderer_process::child) fn deliver_script_fetch(
        &mut self,
        delivery: ScriptFetchDelivery,
        connection: &mut ChildConnection,
    ) -> Result<Option<AdvanceResult>, String> {
        let previous_timer_micros = self.next_timer_micros();
        let (request_id, event, terminal) = match delivery {
            ScriptFetchDelivery::Head { document, head } => {
                if document != self.id {
                    return Ok(None);
                }
                (
                    head.request_id,
                    ScriptFetchEvent::Head(into_fetch_head_result(head)),
                    false,
                )
            }
            ScriptFetchDelivery::Chunk {
                document,
                request_id,
                bytes,
            } => {
                if document != self.id {
                    return Ok(None);
                }
                (request_id, ScriptFetchEvent::Chunk(bytes), false)
            }
            ScriptFetchDelivery::End {
                document,
                request_id,
            } => {
                if document != self.id {
                    return Ok(None);
                }
                (request_id, ScriptFetchEvent::End, true)
            }
            ScriptFetchDelivery::Abort {
                document,
                request_id,
                error,
            } => {
                if document != self.id {
                    return Ok(None);
                }
                (
                    request_id,
                    ScriptFetchEvent::Abort(into_fetch_error(error)),
                    true,
                )
            }
        };
        let Some(script_id) = self.active_script_fetches.get(&request_id).copied() else {
            return Ok(None);
        };
        if terminal {
            self.active_script_fetches.remove(&request_id);
        }

        let started = Instant::now();
        let mut outcome = if let Some(runtime) = self.script_runtime.as_mut() {
            // Fetch promise callbacks may insert several force-async scripts. Leave those queued
            // until the callback task returns so the document clock path can start one bounded
            // concurrent batch instead of blocking this browser-network delivery on each URL.
            runtime.deliver_fetch_event_with_loader(script_id, event, None)
        } else {
            ScriptOutcome::default()
        };
        self.pending_fetches.append(&mut outcome.fetch_actions);
        self.pending_worker_actions
            .append(&mut outcome.worker_actions);
        // Abort and chained Fetch actions produced by a network callback belong to the same
        // networking task. Submit them before accepting the next response chunk so cancellation
        // does not wait for an unrelated clock tick.
        self.start_pending_fetches(connection)?;
        connection.send_state_mutations(self.id, &mut outcome)?;

        let needs_present = outcome.render_requested;
        let next_timer_micros = self.next_timer_micros();
        let reports_runtime_change = outcome.executed != 0
            || outcome.mutation_count != 0
            || !outcome.errors.is_empty()
            || !outcome.console.is_empty()
            || !outcome.diagnostics.is_empty()
            || outcome.navigation_url.is_some()
            || !outcome.history_actions.is_empty()
            || outcome.runtime_stopped
            || !outcome.invalidation.is_empty();
        let style = if needs_present {
            self.page.refresh_resources_after_invalidation_for_viewport(
                self.viewport.style_width,
                self.viewport.height,
                &outcome.invalidation,
            )
        } else {
            StyleRefreshStats::default()
        };
        self.start_presentational_preloads(connection)?;
        let layout_started = Instant::now();
        if needs_present {
            self.rebuild_layout();
        }
        let current_load = self.text.borrow_mut().finish_load_report(PageLoadReport {
            script_micros: micros(started.elapsed()),
            layout_micros: micros(layout_started.elapsed()),
            ..PageLoadReport::default()
        });
        if !terminal
            && !needs_present
            && !reports_runtime_change
            && next_timer_micros == previous_timer_micros
        {
            // Network reads can produce thousands of chunks per second. A quiet callback changes
            // only renderer-owned stream state, so emitting one IPC runtime report per chunk can
            // fill the renderer's output pipe and starve heartbeats. Preserve its metrics and
            // flush them with the next observable or terminal network update.
            self.deferred_network_load =
                std::mem::take(&mut self.deferred_network_load).coalesce(current_load);
            return Ok(None);
        }
        let load = std::mem::take(&mut self.deferred_network_load).coalesce(current_load);
        if needs_present {
            self.presentation(outcome, style, load)
                .map(|presentation| Some(AdvanceResult::Presentation(Box::new(presentation))))
        } else {
            Ok(Some(AdvanceResult::Runtime(Box::new(
                RendererRuntimeUpdate {
                    document: self.id,
                    clock_advanced: false,
                    runtime: runtime_report(
                        outcome,
                        self.script_runtime.is_some(),
                        self.media_runtime_report(),
                    ),
                    load,
                    next_timer_micros,
                },
            ))))
        }
    }
}
