//! Child HTML input uses the same parser and script queues as top-level documents.
use super::*;
use crate::engine::dom::incremental::{HtmlParser, ParserStep};
use crate::engine::page::{PageResource, PageScript};
use crate::engine::script::runtime::parser_queue::ParserScripts;

pub(super) struct FrameDocument {
    pub scripts: bool,
    pub(super) parser: Option<HtmlParser>,
    pub(super) input: Option<super::streaming::Input>,
    pub queue: ParserScripts,
    pub requested: HashSet<PageResource>,
    ordinal: usize,
    pub(super) styles_pending: bool,
    pub(super) styles_load_pending: bool,
}

impl FrameDocument {
    pub fn new(parser: HtmlParser, scripts: bool) -> Self {
        let mut queue = ParserScripts::default();
        queue.set_parsing(true);
        Self {
            scripts,
            parser: Some(parser),
            input: None,
            queue,
            requested: HashSet::new(),
            ordinal: 0,
            styles_pending: false,
            styles_load_pending: false,
        }
    }

    pub fn runnable(&self) -> bool {
        self.queue.has_ready_with_styles(!self.styles_pending)
            || (self.parser.as_ref().is_some_and(HtmlParser::runnable) && !self.queue.blocked())
    }

    pub fn advance(&mut self, runtime: &mut ScriptRuntime) -> ScriptOutcome {
        self.queue.prepare_modules(runtime);
        let script = if self.styles_pending {
            self.queue.pop_nonblocking_ready(false)
        } else {
            self.queue.pop_ready()
        };
        if let Some(script) = script {
            return self.execute(runtime, script);
        }
        let Some(parser) = &mut self.parser else {
            return ScriptOutcome::default();
        };
        let step = parser.advance();
        let mut outcome = runtime.parser_dom_changed(parser.dom().take_parser_mutations());
        runtime.set_quirks_mode(
            parser.dom().quirks_mode.get() != html5ever::tree_builder::QuirksMode::NoQuirks,
        );
        match step {
            ParserStep::Encoding(label) => {
                if let Some(input) = &mut self.input {
                    input.restart = input.decoder.change_encoding(&label);
                }
            }
            ParserStep::Script(node) if self.scripts => {
                self.ordinal += 1;
                if self.ordinal <= crate::limits::MAX_PAGE_SCRIPTS {
                    let base = runtime.host.borrow().script_base_url();
                    if let Some(script) =
                        crate::engine::page::prepare_written_script(node, &base, self.ordinal)
                    {
                        runtime.mark_parser_script_prepared(&script.node);
                        self.queue.enqueue(script);
                    }
                }
            }
            ParserStep::CustomElement(node) => {
                let parser = self.parser.take().unwrap();
                let result = runtime.execute_parser_element(
                    node,
                    parser,
                    self.prepare(runtime),
                    self.ordinal,
                );
                self.parser = Some(result.parser);
                for (script, executed) in result.prepared {
                    if !executed {
                        self.queue.enqueue(script);
                    }
                }
                append(&mut outcome, result.outcome);
            }
            ParserStep::End => {
                self.parser = None;
                self.queue.set_parsing(false);
                runtime.set_deferred_scripts_pending(self.queue.deferred_pending());
                append(&mut outcome, runtime.finish_document_lifecycle());
            }
            _ => (),
        }
        self.update_pending(runtime);
        outcome
    }

    fn execute(&mut self, runtime: &mut ScriptRuntime, script: PageScript) -> ScriptOutcome {
        let blocking = script.blocks_first_paint;
        let graph_error = self.queue.modules.error(&script).map(str::to_owned);
        let Some(code) = script.code.filter(|_| graph_error.is_none()) else {
            let mut result = runtime
                .dispatch_user_input(UserInputEvent::Simple {
                    target: script.node,
                    event_type: "error",
                    bubbles: false,
                    cancelable: false,
                })
                .outcome;
            if let Some(error) = graph_error {
                result.diagnostics.push(error);
            }
            self.update_pending(runtime);
            return result;
        };
        let input = ScriptInput {
            node: script.node,
            source_url: script.source_url,
            code,
            kind: script.kind,
            fetch_options: script.fetch_options,
            finish_lifecycle: false,
        };
        let outcome = if blocking && let Some(parser) = self.parser.take() {
            let result =
                runtime.execute_parser_script(input, parser, self.prepare(runtime), self.ordinal);
            self.parser = Some(result.parser);
            for (script, executed) in result.prepared {
                if !executed {
                    self.queue.enqueue(script);
                }
            }
            result.outcome
        } else {
            runtime.execute_additional_with_loader(&[input], None)
        };
        self.update_pending(runtime);
        outcome
    }

    fn prepare(&self, runtime: &ScriptRuntime) -> parser::PrepareWrittenScript {
        let host = Rc::clone(&runtime.host);
        Rc::new(RefCell::new(move |node, ordinal, _: &[NodeRef]| {
            (
                crate::engine::page::prepare_written_script(
                    node,
                    &host.borrow().script_base_url(),
                    ordinal,
                ),
                false,
            )
        }))
    }

    pub(super) fn update_pending(&mut self, runtime: &mut ScriptRuntime) {
        self.queue.prepare_modules(runtime);
        runtime.set_deferred_scripts_pending(self.queue.deferred_pending());
        runtime.set_document_load_pending(
            self.parser.is_some() || self.queue.is_pending() || self.styles_load_pending,
        );
    }
}

pub(in crate::engine::script::runtime) fn append(
    outcome: &mut ScriptOutcome,
    mut other: ScriptOutcome,
) {
    outcome.executed += other.executed;
    outcome.mutation_count += other.mutation_count;
    outcome.errors.append(&mut other.errors);
    outcome.console.append(&mut other.console);
    outcome.diagnostics.append(&mut other.diagnostics);
    outcome.fetch_actions.append(&mut other.fetch_actions);
    outcome.worker_actions.append(&mut other.worker_actions);
    // A child document's nodes are not parent invalidation roots, but its paint
    // still has to be recomposed when a streamed resource completes.
    outcome.render_requested |= other.render_requested;
    if other.navigation_url.is_some() {
        outcome.navigation_url = other.navigation_url;
        outcome.navigation_options = other.navigation_options;
    }
}
