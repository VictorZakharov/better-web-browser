//! Authoritative HTML input and parser-blocking script pause/resume ownership.
use super::*;
use crate::engine::dom::incremental::{HtmlParser, ParserStep};

pub(super) struct DocumentParser {
    pub(super) parser: HtmlParser,
    pub(super) next: Option<ParserStep>,
}

impl DocumentRuntime {
    pub(super) fn parser_runnable(&self) -> bool {
        self.parser.is_some()
            && (!self.parser_scripts.blocked()
                || (self.parser_scripts.blocking_ready() && !self.parser_stylesheets_pending()))
    }

    pub(super) fn advance_parser(
        &mut self,
        connection: &mut ChildConnection,
        outcome: &mut ScriptOutcome,
    ) -> Result<(), String> {
        let started = Instant::now();
        while self.parser.is_some() {
            if outcome.navigation_url.is_some() {
                break;
            }
            if self.parser_scripts.blocked() {
                if !self.parser_scripts.blocking_ready() || self.parser_stylesheets_pending() {
                    break;
                }
                self.execute_pending_parser_script(connection, outcome)?;
            }
            let parser = self.parser.as_mut().expect("active parser");
            let step = parser
                .next
                .take()
                .unwrap_or_else(|| parser.parser.advance());
            self.page
                .dom
                .quirks_mode
                .set(parser.parser.dom().quirks_mode.get());
            self.page
                .dom
                .errors
                .replace(parser.parser.dom().errors.borrow().clone());
            if let Some(runtime) = self.script_runtime.as_mut() {
                runtime.set_quirks_mode(
                    self.page.dom.quirks_mode.get()
                        != html5ever::tree_builder::QuirksMode::NoQuirks,
                );
                merge_outcome(
                    outcome,
                    runtime.parser_dom_changed(),
                    self.page.dom.document.id(),
                );
            }
            self.page.discover_parsed_resources();
            if let Some(url) = self.page.immediate_refresh_url() {
                outcome.navigation_url = Some(url);
                break;
            }
            outcome.render_requested = true;
            outcome.invalidation =
                crate::engine::invalidation::RenderInvalidation::full(self.page.dom.document.id());
            match step {
                ParserStep::Script(node) => {
                    if let Some(script) = self.page.prepare_parser_script(node) {
                        if let Some(runtime) = self.script_runtime.as_mut() {
                            runtime.mark_parser_script_prepared(&script.node);
                        }
                        self.parser_scripts.enqueue(script);
                    }
                }
                ParserStep::End => {
                    self.parser = None;
                    self.parser_scripts.set_parsing(false);
                    if let Some(runtime) = self.script_runtime.as_mut() {
                        runtime
                            .set_deferred_scripts_pending(self.parser_scripts.deferred_pending());
                        merge_outcome(
                            outcome,
                            runtime.finish_document_lifecycle(),
                            self.page.dom.document.id(),
                        );
                    }
                }
            }
            self.start_presentational_preloads(connection)?;
            self.sync_script_layout_page();
            if started.elapsed() >= Duration::from_millis(8) {
                break;
            }
        }
        Ok(())
    }

    pub(super) fn parser_stylesheets_pending(&self) -> bool {
        self.page.resources.iter().any(|resource| {
            matches!(resource, PageResource::Stylesheet { .. })
                && !self.loaded_resources.contains(resource)
        })
    }
}
