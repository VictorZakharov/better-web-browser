//! Native wheel dispatch latency, distinct from direct retained-scroll probes.
mod browser;
mod json;
#[cfg(test)]
mod tests;

use better_web_browser::renderer_protocol::{DocumentId, WheelAcknowledgement, WheelDecision};
use std::time::{Duration, Instant};

const MAX_SAMPLES: usize = 128;

#[derive(Default)]
pub(in crate::windows_app) struct WheelTrace {
    samples: Vec<Sample>,
    omitted_inputs: u64,
    unmatched_acknowledgements: u64,
}

struct Sample {
    document: DocumentId,
    sequence: u64,
    delta: i32,
    enqueued: Instant,
    decision_received: Option<Duration>,
    dispatch: Option<Duration>,
    decision: Option<WheelDecision>,
    nested_revision: Option<u64>,
    first_paint: Option<Duration>,
    paint_path: Option<&'static str>,
    status: &'static str,
}

impl WheelTrace {
    pub(super) fn enqueue(
        &mut self,
        document: DocumentId,
        sequence: u64,
        delta: i32,
        now: Instant,
    ) {
        self.retire_other_documents(document);
        if self.samples.len() == MAX_SAMPLES {
            self.omitted_inputs = self.omitted_inputs.saturating_add(1);
            return;
        }
        self.samples.push(Sample {
            document,
            sequence,
            delta,
            enqueued: now,
            decision_received: None,
            dispatch: None,
            decision: None,
            nested_revision: None,
            first_paint: None,
            paint_path: None,
            status: "unacknowledged",
        });
    }

    pub(super) fn rejected(&mut self, document: DocumentId, sequence: u64) {
        if let Some(sample) = self
            .samples
            .iter_mut()
            .find(|sample| sample.document == document && sample.sequence == sequence)
        {
            sample.status = "input_rejected";
        }
    }

    pub(super) fn retire_other_documents(&mut self, document: DocumentId) {
        for sample in &mut self.samples {
            if sample.document != document && sample.pending() {
                sample.status = "retired_document";
            }
        }
    }

    pub(in crate::windows_app) fn retire_document(&mut self, document: DocumentId) {
        for sample in &mut self.samples {
            if sample.document == document && sample.pending() {
                sample.status = "retired_document";
            }
        }
    }

    pub(super) fn acknowledge(
        &mut self,
        document: DocumentId,
        values: &[WheelAcknowledgement],
        revision: Option<u64>,
        now: Instant,
    ) {
        for (index, value) in values.iter().enumerate() {
            let Some(sample) = self.samples.iter_mut().find(|sample| {
                sample.document == document
                    && sample.sequence == value.sequence
                    && sample.status == "unacknowledged"
            }) else {
                self.unmatched_acknowledgements = self.unmatched_acknowledgements.saturating_add(1);
                continue;
            };
            sample.decision_received = Some(now.saturating_duration_since(sample.enqueued));
            sample.dispatch = Some(Duration::from_micros(value.dispatch_micros));
            sample.decision = Some(value.decision);
            sample.status = match value.decision {
                WheelDecision::Cancelled => "cancelled",
                WheelDecision::NoMotion => "no_motion",
                // Compaction installs only its latest snapshot/default action. Preserve the
                // earlier verdict but never attribute that later paint to a superseded input.
                _ if index + 1 != values.len() => "superseded",
                WheelDecision::NestedScroll if revision.is_some() => {
                    sample.nested_revision = revision;
                    "awaiting_nested_paint"
                }
                WheelDecision::NestedScroll => "missing_nested_presentation",
                WheelDecision::Viewport => "awaiting_viewport_paint",
            };
        }
    }

    pub(super) fn wants_viewport_paint(&self, document: DocumentId) -> bool {
        self.samples
            .iter()
            .any(|sample| sample.document == document && sample.status == "awaiting_viewport_paint")
    }

    pub(super) fn viewport_request(&mut self, document: DocumentId, new_motion: bool) {
        if !new_motion {
            self.no_motion(document, None);
        }
    }

    pub(super) fn wants_nested_paint(&self, document: DocumentId, revision: u64) -> bool {
        self.samples.iter().any(|sample| {
            sample.document == document
                && sample.status == "awaiting_nested_paint"
                && sample.nested_revision == Some(revision)
        })
    }

    pub(super) fn painted(
        &mut self,
        document: DocumentId,
        revision: Option<u64>,
        now: Instant,
        full_repaint: bool,
    ) {
        for sample in &mut self.samples {
            let expected = if let Some(revision) = revision {
                sample.status == "awaiting_nested_paint" && sample.nested_revision == Some(revision)
            } else {
                sample.status == "awaiting_viewport_paint"
            };
            if sample.document == document && expected {
                sample.first_paint = Some(now.saturating_duration_since(sample.enqueued));
                sample.paint_path = Some(if full_repaint {
                    "full_retained"
                } else {
                    "exposed_strip"
                });
                sample.status = "painted";
            }
        }
    }

    pub(super) fn no_motion(&mut self, document: DocumentId, revision: Option<u64>) {
        for sample in &mut self.samples {
            if sample.document == document
                && ((revision.is_none() && sample.status == "awaiting_viewport_paint")
                    || (revision.is_some()
                        && sample.status == "awaiting_nested_paint"
                        && sample.nested_revision == revision))
            {
                sample.status = "no_motion";
            }
        }
    }

    pub(super) fn paint_failed(&mut self, document: DocumentId, revision: Option<u64>) {
        for sample in &mut self.samples {
            if sample.document == document
                && ((revision.is_none() && sample.status == "awaiting_viewport_paint")
                    || (revision.is_some()
                        && sample.nested_revision == revision
                        && sample.status == "awaiting_nested_paint"))
            {
                sample.status = "paint_failed";
            }
        }
    }
}

impl Sample {
    fn pending(&self) -> bool {
        matches!(
            self.status,
            "unacknowledged" | "awaiting_nested_paint" | "awaiting_viewport_paint"
        )
    }
}
