//! Process-thread V8 tester for HTML `pattern` validation.
//!
//! Constraint validation needs Unicode-aware ECMAScript pattern semantics
//! (`RegExpCreate(pattern, 'v')`, whole-value anchored) everywhere the
//! validity algorithm runs: scripted documents, selector matching during
//! style refresh, and scriptless documents that own no author realm. A Rust
//! regex dialect would disagree with the platform on Unicode sets and
//! property escapes, so the single validity algorithm in
//! [`crate::engine::dom`] calls back into V8 through this narrow boundary.
//!
//! Each thread owns one lightweight isolate with no DOM, no host callbacks,
//! and no author-reachable state: it only compiles anchored patterns and runs
//! `RegExp.prototype.test`. Callers pass owned strings across the boundary
//! and must not hold DOM borrows while calling in, so evaluation can never
//! re-enter page script under a borrow. Compiled patterns are cached per
//! source with a small bound; invalid patterns compile to nothing and impose
//! no constraint, per the HTML `pattern` attribute rules.
//!
//! Like author-script regexes, pathological patterns can take a while to test.
//! They share the process rather than the page watchdog; keep patterns small
//! and report hangs with the pattern that caused them.

use std::cell::RefCell;
use std::collections::HashMap;

/// Bound on cached compiled patterns; the cache is dropped whole when full.
const MAX_CACHED_PATTERNS: usize = 64;

struct PatternEngine {
    isolate: v8::OwnedIsolate,
    context: Option<v8::Global<v8::Context>>,
    compiled: HashMap<String, v8::Global<v8::RegExp>>,
}

impl PatternEngine {
    fn fresh() -> Self {
        crate::engine::script::engine::runtime::initialize_v8();
        Self {
            isolate: v8::Isolate::new(v8::CreateParams::default()),
            context: None,
            compiled: HashMap::new(),
        }
    }
}

thread_local! {
    static ENGINE: RefCell<PatternEngine> = RefCell::new(PatternEngine::fresh());
}

/// Whether a page isolate is currently entered on this thread. Evaluating on
/// this engine's own isolate while a page isolate is entered would nest
/// isolates and crash; Rust-side pattern warming checks this flag and defers
/// to the calling realm's own evaluation instead. The flag is maintained by
/// the script engine's isolate-entry guard, so it is exact by construction.
pub(crate) fn page_isolate_entered() -> bool {
    ENTERED.with(|flag| flag.get())
}

pub(crate) fn set_page_isolate_entered(entered: bool) -> bool {
    ENTERED.with(|flag| flag.replace(entered))
}

thread_local! {
    static ENTERED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
/// Tests `value` against an HTML `pattern` attribute.
///
/// The pattern is anchored (`^(?:pattern)$`) and compiled with Unicode-sets
/// semantics. Returns `None` when the pattern does not compile; callers treat
/// that as "no pattern constraint", never as a match or a mismatch.
///
/// Never call this while a page isolate is entered on this thread (for
/// example from inside a host callback); use [`eval_with_scope`] on the
/// calling realm's scope instead.
pub(crate) fn test_pattern(pattern: &str, value: &str) -> Option<bool> {
    ENGINE.with(|cell| {
        let mut engine = cell.borrow_mut();
        let PatternEngine {
            isolate,
            context,
            compiled,
        } = &mut *engine;
        if context.is_none() {
            v8::scope!(let scope, isolate);
            let fresh = v8::Context::new(scope, Default::default());
            *context = Some(v8::Global::new(scope, fresh));
        }
        let shared = context.clone().expect("pattern context initialized");
        v8::scope!(let scope, isolate);
        let context = v8::Local::new(scope, &shared);
        let scope = &mut v8::ContextScope::new(scope, context);
        // Contain failed compiles: a pending SyntaxError must never poison
        // later evaluations sharing this isolate.
        v8::tc_scope!(let tc, scope);
        let expression = if let Some(cached) = compiled.get(pattern) {
            v8::Local::new(tc, cached)
        } else {
            // Validity is judged on the raw pattern: wrapping an invalid
            // pattern in `^(?:...)$` can accidentally balance it (the
            // wrapper's `(` pairs with a stray `)`, e.g. `a)(b`), so the
            // anchored test must never run for patterns that fail this
            // check. HTML constrains only v-compilable patterns.
            let raw = v8::String::new(tc, pattern)?;
            if v8::RegExp::new(tc, raw, v8::RegExpCreationFlags::UNICODE_SETS).is_none() {
                tc.reset();
                return None;
            }
            let source = v8::String::new(tc, &format!("^(?:{pattern})$"))?;
            let expression =
                match v8::RegExp::new(tc, source, v8::RegExpCreationFlags::UNICODE_SETS) {
                    Some(expression) => expression,
                    None => {
                        tc.reset();
                        return None;
                    }
                };
            if compiled.len() >= MAX_CACHED_PATTERNS {
                compiled.clear();
            }
            compiled.insert(pattern.to_string(), v8::Global::new(tc, expression));
            expression
        };
        let name = v8::String::new(tc, "test")?;
        let test = v8::Local::<v8::Function>::try_from(expression.get(tc, name.into())?).ok()?;
        let argument = v8::String::new(tc, value)?;
        let outcome = test.call(tc, expression.into(), &[argument.into()])?;
        Some(outcome.boolean_value(tc))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dormant_realm_probe() {
        use crate::engine::dom;
        use crate::engine::script::{ScriptFetchOptions, ScriptInput, ScriptKind, ScriptRuntime};
        let dom = dom::parse_with_scripting("<input pattern='a+'><script>1+1</script>", true);
        let script = dom.elements_named("script").next().unwrap();
        let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
        // Pre-first-task: the fresh isolate is entered but has never run.
        assert_eq!(test_pattern("a+", "aaa"), Some(true));
        let result = runtime.execute_initial(&[ScriptInput {
            source_url: "https://example.com/".into(),
            code: script.text_content(),
            node: script,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        }]);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        // Post-task dormant with the entry guard dropped.
        let verdict = test_pattern("a+", "aaa");
        assert_eq!(verdict, Some(true));
    }

    #[test]
    fn anchored_unicode_pattern_semantics() {
        assert_eq!(test_pattern("[a-z]+", "abc"), Some(true));
        assert_eq!(test_pattern("[a-z]+", "ab1"), Some(false));
        // Whole-value anchoring: partial matches do not count.
        assert_eq!(test_pattern("a|ab", "ab"), Some(true));
        assert_eq!(test_pattern("a", "ab"), Some(false));
        // Unicode sets mode: property escapes and set operations work.
        assert_eq!(test_pattern(r"[\p{Letter}]+", "Äöü"), Some(true));
        assert_eq!(test_pattern(r"[\p{Number}--[0-9]]", "٣"), Some(true));
        // Invalid patterns impose no constraint instead of throwing.
        assert_eq!(test_pattern("([", "([)"), None);
        assert_eq!(test_pattern("a)(b", "de"), None);
        assert_eq!(test_pattern("[(]", "x"), None);
        assert_eq!(test_pattern("", ""), Some(true));
    }
}
