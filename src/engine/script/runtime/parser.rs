//! Incremental parser notifications for the retained document realm.
use super::*;
impl ScriptRuntime {
    pub(crate) fn set_parser_write_capture(&mut self, capture: bool) {
        self.host.borrow_mut().capture_parser_writes = capture;
    }

    pub(crate) fn take_parser_writes(&mut self) -> String {
        let mut host = self.host.borrow_mut();
        host.capture_parser_writes = false;
        std::mem::take(&mut host.pending_document_write)
    }

    pub(crate) fn parser_dom_changed(&mut self) -> ScriptOutcome {
        let document = self.host.borrow().document.clone();
        let ids = {
            let mut host = self.host.borrow_mut();
            host.begin_task();
            // Parser mutations bypass JS mutation recording. Invalidate the independent CSSOM
            // and synchronous-layout caches before constructors or the next script can query
            // newly inserted nodes/sheets; presentation invalidation alone arrives too late.
            host.computed_styles = None;
            host.offset_parent_styles = None;
            host.layout_geometry_initialized = false;
            host.pending_layout_invalidation.record(
                &document,
                Some(&document),
                MutationKind::Stylesheet,
            );
            let new_elements = Node::descendants(&document)
                .filter(|node| node.element().is_some() && !host.node_ids.contains_key(&node.id()))
                .collect::<Vec<_>>();
            host.register_subtree(&document);
            new_elements
                .iter()
                .map(|node| host.id_for(node).to_string())
                .collect::<Vec<_>>()
                .join(",")
        };
        let Some(context) = self.context.as_deref_mut() else {
            return inactive_runtime_outcome();
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut outcome = ScriptOutcome::default();
            if let Err(error) = context.call_global("__parserDomChanged", &[JsValue::from(ids)]) {
                outcome
                    .errors
                    .push(format!("parser DOM notification: {error}"));
            }
            if let Err(error) = context.run_jobs() {
                outcome.errors.push(format!("parser promise jobs: {error}"));
            }
            outcome
        }));
        self.finish_guarded_run(result)
    }
}
