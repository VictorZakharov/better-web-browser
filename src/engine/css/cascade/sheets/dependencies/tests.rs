use super::*;
use crate::engine::css::selector_parser::parse_selector;
use crate::engine::invalidation::{InvalidationImpact, MutationKind};

fn dependencies(selector: &str) -> NonlocalDependencies {
    let selector = parse_selector(selector).expect("valid dependency test selector");
    let mut dependencies = NonlocalDependencies::default();
    dependencies.include(&selector);
    dependencies
}

#[test]
fn relative_style_attributes_widen_even_inside_functional_and_nth_filters() {
    for selector in [
        "html:has([style])",
        "html:has([STYLE^='padding' i])",
        ":is(html:has([style])) body",
        "html:not(:not(:has([style]))) body",
        "body:nth-child(1 of :has([style]))",
        "html:has(:is(.a, [style]))",
        "html:has(:not([style]))",
        "html:has(:nth-child(1 of [style]))",
        "html:has(:nth-last-child(2 of :where([style])))",
        ".earlier:has(+ .later [style])",
        ".earlier:has(~ .later:is([style]))",
    ] {
        let dependencies = dependencies(selector);
        assert!(
            dependencies.any && dependencies.style_attribute,
            "{selector}"
        );
        assert!(
            dependencies.needs_document_refresh(MutationKind::Attribute("style").impact()),
            "{selector}"
        );
        // Overlay writes do not modify attributes, but coalescing can also include
        // an inline write. Their shared declaration-only contract is conservative.
        assert!(dependencies.needs_document_refresh(MutationKind::StyleOverlay.impact()));
    }
}

#[test]
fn unrelated_relational_membership_does_not_widen_declaration_only_writes() {
    for selector in [
        "html:has(.active) body",
        ":where(html:has(.active)) body",
        "html[style]:has(.active) body",
        "html:has([class]) body[style]",
        "html:has(:checked)",
        "html:has(:focus-within)",
        "html:has(:nth-child(1 of .active))",
    ] {
        let dependencies = dependencies(selector);
        assert!(
            dependencies.any && !dependencies.style_attribute,
            "{selector}"
        );
        assert!(!dependencies.needs_document_refresh(MutationKind::Attribute("style").impact()));
        assert!(!dependencies.needs_document_refresh(MutationKind::StyleOverlay.impact()));
        for impact in [
            InvalidationImpact::default(),
            MutationKind::Attribute("class").impact(),
            MutationKind::ChildList.impact(),
            MutationKind::State.impact(),
            MutationKind::StyleOverlay
                .impact()
                .union(MutationKind::CharacterData.impact()),
        ] {
            assert!(
                dependencies.needs_document_refresh(impact),
                "{selector}: {impact:?}"
            );
        }
    }
}

#[test]
fn local_attribute_selectors_do_not_create_nonlocal_edges() {
    for selector in [
        "[style]",
        "[style] + .next",
        ".parent [style] .child",
        ".child:nth-child(1 of [style])",
    ] {
        let dependencies = dependencies(selector);
        assert!(
            !dependencies.any && !dependencies.style_attribute,
            "{selector}"
        );
    }
}

#[test]
fn empty_rule_set_has_no_nonlocal_dependencies() {
    assert!(
        !NonlocalDependencies::for_rules(&[]).needs_document_refresh(InvalidationImpact::STYLE)
    );
}
