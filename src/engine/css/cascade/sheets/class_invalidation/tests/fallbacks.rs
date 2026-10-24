//! Changes that invalidation sets cannot bound are counted by reason.
use super::*;
use crate::engine::css::InvalidationFallbacks;

#[test]
fn unbounded_class_changes_count_why_they_restyle_whole_subtrees() {
    let none = InvalidationFallbacks::default();
    for (css, expected) in [
        (".open .foo{color:red}", none),
        (
            ".open + body p{color:red}",
            InvalidationFallbacks {
                sibling_combinator: 1,
                ..none
            },
        ),
        (
            ":is(.open section) p{color:red}",
            InvalidationFallbacks {
                nested_selector: 1,
                ..none
            },
        ),
        (
            "section:has(.open){color:red}",
            InvalidationFallbacks {
                nested_selector: 1,
                ..none
            },
        ),
        (
            ":is([class*=op] section) p{color:red}",
            InvalidationFallbacks {
                unbounded_rules: 1,
                ..none
            },
        ),
        (
            ".open *{color:red}",
            InvalidationFallbacks {
                keyless_descendant: 1,
                ..none
            },
        ),
    ] {
        let stats = change_class(&page(css, "closed"), "open");
        assert_eq!(stats.invalidation_fallbacks, expected, "{css}");
    }
}
