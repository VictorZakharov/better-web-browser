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

/// Tests `value` against an HTML `pattern` attribute.
///
/// The pattern is anchored (`^(?:pattern)$`) and compiled with Unicode-sets
/// semantics. Returns `None` when the pattern does not compile; callers treat
/// that as "no pattern constraint", never as a match or a mismatch.
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
        let expression = if let Some(cached) = compiled.get(pattern) {
            v8::Local::new(scope, cached)
        } else {
            let source = v8::String::new(scope, &format!("^(?:{pattern})$"))?;
            let expression = v8::RegExp::new(scope, source, v8::RegExpCreationFlags::UNICODE_SETS)?;
            if compiled.len() >= MAX_CACHED_PATTERNS {
                compiled.clear();
            }
            compiled.insert(pattern.to_string(), v8::Global::new(scope, expression));
            expression
        };
        let name = v8::String::new(scope, "test")?;
        let test = v8::Local::<v8::Function>::try_from(expression.get(scope, name.into())?).ok()?;
        let argument = v8::String::new(scope, value)?;
        let outcome = test.call(scope, expression.into(), &[argument.into()])?;
        Some(outcome.boolean_value(scope))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(test_pattern("", ""), Some(true));
    }
}
