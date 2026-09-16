//! Embedder handoff for script-created parsers; the realm survives document.open().
use super::parser::PrepareWrittenScript;
use super::*;
use crate::engine::page::PageScript;

pub(crate) struct StreamUpdate {
    pub replaced: bool,
    pub parsing: bool,
    pub quirks: html5ever::tree_builder::QuirksMode,
    pub mutated: bool,
    pub scripts: Vec<(PageScript, bool)>,
    pub stylesheets: Vec<NodeRef>,
}

impl ScriptRuntime {
    pub(crate) fn set_stream_preparation(&mut self, prepare: PrepareWrittenScript) {
        let mut host = self.host.borrow_mut();
        host.document_streams.remaining_script_bytes =
            Some(MAX_PAGE_SCRIPT_BYTES.saturating_sub(self.total_script_bytes));
        let id = host.document.id();
        if let Some(session) = host.document_streams.parsers.get_mut(&id) {
            session.prepare = prepare.clone();
        }
        host.document_streams.prepare = Some(prepare);
    }

    pub(crate) fn take_stream_update(&mut self) -> Option<StreamUpdate> {
        let mut host = self.host.borrow_mut();
        let id = host.document.id();
        let replaced = std::mem::take(&mut host.document_streams.replaced);
        let session = host.document_streams.parsers.get_mut(&id)?;
        let scripts = std::mem::take(&mut session.prepared);
        session.initial_count += scripts.len();
        self.total_script_bytes += std::mem::take(&mut session.script_bytes);
        session.remaining_script_bytes =
            MAX_PAGE_SCRIPT_BYTES.saturating_sub(self.total_script_bytes);
        Some(StreamUpdate {
            replaced,
            parsing: !session.parser.ended(),
            quirks: session.parser.dom().quirks_mode.get(),
            mutated: std::mem::take(&mut session.mutated),
            scripts,
            stylesheets: std::mem::take(&mut session.stylesheets),
        })
    }

    pub(crate) fn resume_document_stream(&mut self) -> ScriptOutcome {
        let id = {
            let mut host = self.host.borrow_mut();
            let document = host.document.clone();
            let Some(session) = host.document_streams.parsers.get_mut(&document.id()) else {
                return ScriptOutcome::default();
            };
            if session.parser.ended() {
                return ScriptOutcome::default();
            }
            session.paused = false;
            session.blocked_on = None;
            host.id_for(&document)
        };
        let Some(context) = self.context.as_deref_mut() else {
            return inactive_runtime_outcome();
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut outcome = ScriptOutcome::default();
            if let Err(error) = context.call_global("__resumeDocumentStream", &[JsValue::from(id)])
            {
                outcome
                    .errors
                    .push(format!("document stream resumption: {error}"));
            }
            outcome
        }));
        self.finish_guarded_run(result)
    }
}
