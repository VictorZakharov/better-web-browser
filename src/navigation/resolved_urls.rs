//! Recently resolved URLs.
//
// Resolving is a pure function of a base URL and a reference, and a page asks the same questions
// constantly: its images and links are read again after every document change, and an article's
// few hundred distinct references were about 3% of the renderer's work while it loaded, because
// each answer parses the base URL again. Answers are kept per thread for the last base URL only.
// References are page-controlled, so what is kept is bounded in count and size.
use std::cell::RefCell;
use std::collections::HashMap;

const MAX_ENTRIES: usize = 4096;
const MAX_REFERENCE_BYTES: usize = 2048;

#[derive(Default)]
struct ResolvedUrls {
    base: String,
    answers: HashMap<String, Option<String>>,
}

thread_local! {
    static RESOLVED: RefCell<ResolvedUrls> = RefCell::default();
}

/// `resolve()`'s answer for `reference` against `base`, which it must be a pure function of.
pub(super) fn get_or_resolve(
    base: &str,
    reference: &str,
    resolve: impl FnOnce() -> Option<String>,
) -> Option<String> {
    if reference.len() > MAX_REFERENCE_BYTES {
        return resolve();
    }
    RESOLVED.with_borrow_mut(|resolved| {
        if resolved.base != base {
            resolved.base.clear();
            resolved.base.push_str(base);
            resolved.answers.clear();
        }
        if let Some(answer) = resolved.answers.get(reference) {
            return answer.clone();
        }
        let answer = resolve();
        if resolved.answers.len() == MAX_ENTRIES {
            resolved.answers.clear();
        }
        resolved
            .answers
            .insert(reference.to_owned(), answer.clone());
        answer
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn counted(base: &str, reference: &str, calls: &Cell<u32>, answer: &str) -> Option<String> {
        get_or_resolve(base, reference, || {
            calls.set(calls.get() + 1);
            Some(answer.to_owned())
        })
    }

    #[test]
    fn a_repeated_question_is_answered_once() {
        let calls = Cell::new(0);
        assert_eq!(
            counted("https://a.test/", "x", &calls, "1").as_deref(),
            Some("1")
        );
        assert_eq!(
            counted("https://a.test/", "x", &calls, "2").as_deref(),
            Some("1")
        );
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn another_base_gets_its_own_answer() {
        let calls = Cell::new(0);
        assert_eq!(
            counted("https://a.test/", "x", &calls, "a").as_deref(),
            Some("a")
        );
        assert_eq!(
            counted("https://b.test/", "x", &calls, "b").as_deref(),
            Some("b")
        );
        assert_eq!(
            counted("https://a.test/", "x", &calls, "c").as_deref(),
            Some("c")
        );
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn failures_are_remembered_and_long_references_are_not() {
        let calls = Cell::new(0);
        let fail = |reference: &str| {
            get_or_resolve("https://c.test/", reference, || {
                calls.set(calls.get() + 1);
                None
            })
        };
        assert_eq!(fail("bad"), None);
        assert_eq!(fail("bad"), None);
        assert_eq!(calls.get(), 1);
        let long = "x".repeat(MAX_REFERENCE_BYTES + 1);
        assert_eq!(fail(&long), None);
        assert_eq!(fail(&long), None);
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn what_is_kept_stays_bounded() {
        for index in 0..=MAX_ENTRIES + 1 {
            get_or_resolve("https://d.test/", &index.to_string(), || None);
        }
        RESOLVED.with_borrow(|resolved| assert!(resolved.answers.len() <= MAX_ENTRIES));
    }
}
