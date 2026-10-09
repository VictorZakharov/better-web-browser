//! Embedder-only bounded allocation attribution. Never exposed by getParameter.
use super::{BackendContexts, Kind, WebGl, gl};
use std::cell::Cell;

#[derive(Clone, Copy)]
struct Failure {
    charged: usize,
    additional: usize,
    limit: usize,
    kind: Option<Kind>,
    process_private: Option<Option<usize>>,
}

#[derive(Default)]
pub(super) struct Ledger {
    rejections: Cell<u64>,
    first: Cell<Option<Failure>>,
    latest: Cell<Option<Failure>>,
}

impl Ledger {
    #[cfg(test)]
    pub(super) fn reject(&self, charged: usize, additional: usize, limit: usize) -> u32 {
        self.reject_for(None, charged, additional, limit)
    }

    fn reject_for(
        &self,
        kind: Option<Kind>,
        charged: usize,
        additional: usize,
        limit: usize,
    ) -> u32 {
        let failure = Failure {
            charged,
            additional,
            limit,
            kind,
            process_private: None,
        };
        self.rejections.set(self.rejections.get().saturating_add(1));
        if self.first.get().is_none() {
            self.first.set(Some(failure));
        }
        self.latest.set(Some(failure));
        gl::OUT_OF_MEMORY
    }

    fn reject_headroom(
        &self,
        kind: Option<Kind>,
        charged: usize,
        additional: usize,
        limit: usize,
        private: Option<usize>,
    ) -> u32 {
        let result = self.reject_for(kind, charged, additional, limit);
        let mut failure = self.latest.get().expect("reject_for records a failure");
        failure.process_private = Some(private);
        self.latest.set(Some(failure));
        if self.rejections.get() == 1 {
            self.first.set(Some(failure));
        }
        result
    }
}

impl WebGl {
    pub(super) fn admit_storage_growth(&self, additional: usize) -> super::Result<usize> {
        self.admit_storage_growth_for(None, additional)
    }

    pub(super) fn admit_storage_growth_for(
        &self,
        kind: Option<Kind>,
        additional: usize,
    ) -> super::Result<usize> {
        let counter = self
            .resource_bytes
            .checked_add(additional)
            .filter(|counter| *counter <= self.resource_limit)
            .ok_or_else(|| {
                self.resource_diagnostics.reject_for(
                    kind,
                    self.resource_bytes,
                    additional,
                    self.resource_limit,
                )
            })?;
        if additional != 0 {
            let private = crate::process_memory::current().map(|sample| sample.private);
            if !super::process_headroom::admits(self.resource_bytes, additional, private) {
                return Err(self.resource_diagnostics.reject_headroom(
                    kind,
                    self.resource_bytes,
                    additional,
                    self.resource_limit,
                    private,
                ));
            }
        }
        Ok(counter)
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
                let kind = match failure.kind {
                    Some(Kind::Buffer) => "buffer",
                    Some(Kind::Shader) => "shader",
                    Some(Kind::Texture) => "texture",
                    Some(Kind::Renderbuffer) => "renderbuffer",
                    _ => "other",
                };
                let _ = write!(message, " {label}-rejection-kind={kind}");
                if let Some(private) = failure.process_private {
                    let value =
                        private.map_or_else(|| "unavailable".into(), |bytes| bytes.to_string());
                    let _ = write!(
                        message,
                        " {label}-rejection-domain=process-headroom process-private={value}"
                    );
                } else {
                    let _ = write!(message, " {label}-rejection-domain=resource-budget");
                }
            }
        }
        message
    }
}

impl BackendContexts {
    pub(super) fn resource_diagnostics(&mut self, ids: &[u32]) -> Vec<String> {
        // The caller's live set is bounded, and the owner performs no GL call,
        // getError, readback, or author-state mutation. Only private timing counters drain.
        let mut reports = Vec::new();
        for id in ids.iter().take(super::MAX_CONTEXTS) {
            if let Some(context) = self.contexts.get_mut(id) {
                reports.push(context.resource_diagnostic(*id));
                reports.extend(context.execution_profile.take(*id));
            }
        }
        reports
    }
}

#[cfg(test)]
mod tests;
