//! Embedder-only bounded allocation attribution. Never exposed by getParameter.
use super::{BackendContexts, WebGl, gl};
use std::cell::Cell;

#[derive(Clone, Copy)]
struct Failure {
    charged: usize,
    additional: usize,
    limit: usize,
}

#[derive(Default)]
pub(super) struct Ledger {
    rejections: Cell<u64>,
    first: Cell<Option<Failure>>,
    latest: Cell<Option<Failure>>,
}

impl Ledger {
    pub(super) fn reject(&self, charged: usize, additional: usize, limit: usize) -> u32 {
        let failure = Failure {
            charged,
            additional,
            limit,
        };
        self.rejections.set(self.rejections.get().saturating_add(1));
        if self.first.get().is_none() {
            self.first.set(Some(failure));
        }
        self.latest.set(Some(failure));
        gl::OUT_OF_MEMORY
    }
}

impl WebGl {
    pub(super) fn admit_storage_growth(&self, additional: usize) -> super::Result<usize> {
        self.resource_bytes
            .checked_add(additional)
            .filter(|counter| *counter <= self.resource_limit)
            .ok_or_else(|| {
                self.resource_diagnostics.reject(
                    self.resource_bytes,
                    additional,
                    self.resource_limit,
                )
            })
    }

    fn resource_diagnostic(&self, id: u32) -> String {
        let (objects, pending, capacities) = self.objects.storage_summary();
        let mut message = format!(
            "WebGL {id}: charged={} limit={} surface={} objects={} pending-delete={} capacities(buffer/texture/renderbuffer/shader)={}/{}/{}/{} admission-rejections={}",
            self.resource_bytes,
            self.resource_limit,
            self.surface.bytes(),
            objects,
            pending,
            capacities[0],
            capacities[1],
            capacities[2],
            capacities[3],
            self.resource_diagnostics.rejections.get(),
        );
        for (label, failure) in [
            ("first", self.resource_diagnostics.first.get()),
            ("latest", self.resource_diagnostics.latest.get()),
        ] {
            if let Some(failure) = failure {
                use std::fmt::Write;
                let _ = write!(
                    message,
                    " {label}-rejection(charged/additional/limit)={}/{}/{}",
                    failure.charged, failure.additional, failure.limit
                );
            }
        }
        message
    }
}

impl BackendContexts {
    pub(super) fn resource_diagnostics(&self, ids: &[u32]) -> Vec<String> {
        // The caller's live set is bounded, and the owner performs no GL call,
        // getError, readback, or mutation while collecting this snapshot.
        ids.iter()
            .take(super::MAX_CONTEXTS)
            .filter_map(|id| {
                self.contexts
                    .get(id)
                    .map(|context| context.resource_diagnostic(*id))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
