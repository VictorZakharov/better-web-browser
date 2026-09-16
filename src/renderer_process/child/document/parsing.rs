//! Authoritative HTML input and parser-blocking script pause/resume ownership.
use super::*;
use crate::engine::dom::Node;
use crate::engine::dom::incremental::{HtmlParser, ParserStep};

pub(super) struct DocumentParser {
    pub(super) parser: HtmlParser,
    pub(super) next: Option<ParserStep>,
}

impl DocumentRuntime {
    pub(super) fn parser_runnable(&self) -> bool {
        self.parser
            .as_ref()
            .is_some_and(|parser| parser.next.is_some() || parser.parser.runnable())
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
            if !self.parser_runnable() {
                break;
            }
            if outcome.navigation_url.is_some() {
                break;
            }
            if self.parser_scripts.blocked() {
                if !self.parser_scripts.blocking_ready() || self.parser_stylesheets_pending() {
                    break;
                }
                self.execute_pending_parser_script(connection, outcome)?;
                if self.parser_scripts.blocked() {
                    self.start_presentational_preloads(connection)?;
                    if started.elapsed() >= Duration::from_millis(8) {
                        break;
                    }
                    continue;
                }
            }
            self.update_render_blockers();
            let previous_links = self
                .stylesheet_nodes()
                .iter()
                .map(|node| (node.id(), node.subtree_mutation_version()))
                .collect::<Vec<_>>();
            let parser = self.parser.as_mut().expect("active parser");
            let pending_checkpoint = parser.next.is_some();
            let previous_version = parser.parser.dom().mutation_version();
            let previous_quirks = parser.parser.dom().quirks_mode.get();
            let step = parser
                .next
                .take()
                .unwrap_or_else(|| parser.parser.advance());
            let dom_changed = pending_checkpoint
                || previous_version != parser.parser.dom().mutation_version()
                || previous_quirks != parser.parser.dom().quirks_mode.get();
            self.page
                .dom
                .quirks_mode
                .set(parser.parser.dom().quirks_mode.get());
            self.page
                .dom
                .errors
                .replace(parser.parser.dom().errors.borrow().clone());
            if dom_changed {
                self.page.discover_parsed_resources();
                self.record_parser_stylesheets(&previous_links);
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
                if let Some(url) = self.page.immediate_refresh_url() {
                    outcome.navigation_url = Some(url);
                    break;
                }
                outcome.render_requested = true;
                outcome.invalidation = crate::engine::invalidation::RenderInvalidation::full(
                    self.page.dom.document.id(),
                );
            }
            match step {
                ParserStep::Encoding(label) => {
                    self.parser_encoding(&label);
                    if self.encoding_restart_pending() {
                        break;
                    }
                }
                ParserStep::NeedInput => {}
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
                    if self
                        .navigation
                        .as_ref()
                        .is_some_and(|input| input.decoder.ended())
                    {
                        self.navigation = None;
                    }
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

    pub(super) fn written_script_preparation(
        &self,
    ) -> crate::engine::script::runtime::parser::PrepareWrittenScript {
        let page = Rc::clone(&self.script_layout_page);
        let loaded = self
            .loaded_resources
            .iter()
            .filter(|resource| matches!(resource, PageResource::Stylesheet { .. }))
            .cloned()
            .collect::<HashSet<_>>();
        let admitted = self
            .page
            .resources
            .iter()
            .filter(|resource| matches!(resource, PageResource::Stylesheet { .. }))
            .cloned()
            .collect::<Vec<_>>();
        Box::new(move |node, ordinal, stylesheets| {
            let mut page = page.borrow_mut();
            let script = page.prepare_parser_script_at(node, ordinal);
            let blocked = stylesheets
                .iter()
                .filter(|node| Node::tree_root(node).id() == page.dom.document.id())
                .any(|node| super::rendering::stylesheet_pending(&page, node, &loaded, &admitted));
            (script, blocked)
        })
    }
}
