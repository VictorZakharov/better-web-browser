//! Parser-inserted import maps register in document order before later module graphs.
//! https://html.spec.whatwg.org/multipage/scripting.html#prepare-the-script-element
use super::*;

impl ScriptRuntime {
    pub(crate) fn install_import_map(
        &self,
        source: &str,
        base_url: &str,
    ) -> Result<Vec<String>, String> {
        self.host
            .borrow()
            .module_loader
            .install_import_map(source, base_url)
    }

    /// Returns `None` for another script type and consumes a parser import map otherwise.
    pub(crate) fn process_parser_import_map(&mut self, node: &NodeRef) -> Option<ScriptOutcome> {
        if node.namespace_uri() != Some("http://www.w3.org/1999/xhtml")
            || !node.attr("type").is_some_and(|kind| {
                kind.trim_matches(|character| {
                    matches!(character, '\t' | '\n' | '\x0c' | '\r' | ' ')
                })
                .eq_ignore_ascii_case("importmap")
            })
        {
            return None;
        }
        let element = node.element()?;
        let mut outcome = ScriptOutcome::default();
        if element.script_started.get() {
            return Some(outcome);
        }
        let source = node.text_content();
        if source.is_empty() && node.attr("src").is_none() {
            return Some(outcome);
        }
        self.mark_parser_script_prepared(node);
        let (allowed, base_url) = {
            let host = self.host.borrow();
            (
                !host.sandbox.scripts_blocked
                    && host.policy.allows_inline_script(
                        node.attr("nonce").as_deref(),
                        &source,
                        element.script_parser_inserted.get(),
                    ),
                host.script_base_url(),
            )
        };
        if node.attr("src").is_none() && !allowed {
            let input = ScriptInput {
                node: node.clone(),
                source_url: base_url,
                code: source,
                kind: ScriptKind::Classic,
                fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
                finish_lifecycle: false,
            };
            if let Some(context) = self.context.as_deref_mut() {
                super::super::execution::csp_reporting::queue_script_violation(
                    context,
                    &self.host,
                    &mut outcome,
                    &input,
                    false,
                );
            }
            outcome
                .diagnostics
                .push("import map blocked by document policy".into());
            return Some(outcome);
        }
        let result = if node.attr("src").is_some() {
            Err("import maps cannot use a src attribute".into())
        } else {
            self.install_import_map(&source, &base_url)
        };
        match result {
            Ok(diagnostics) => outcome.diagnostics.extend(diagnostics),
            Err(error) => {
                outcome = self
                    .dispatch_user_input(UserInputEvent::Simple {
                        target: node.clone(),
                        event_type: "error",
                        bubbles: false,
                        cancelable: false,
                    })
                    .outcome;
                outcome.diagnostics.push(format!("import map: {error}"));
            }
        }
        Some(outcome)
    }
}
