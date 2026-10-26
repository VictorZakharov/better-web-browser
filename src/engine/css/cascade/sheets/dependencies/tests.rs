use super::*;
use crate::engine::css::selector_parser::parse_selector;
use crate::engine::invalidation::{InvalidationImpact, MutationKind};

fn dependencies(selector: &str) -> NonlocalDependencies {
    let selector = parse_selector(selector).expect("valid dependency test selector");
    let mut dependencies = NonlocalDependencies::default();
    dependencies.include(&mut vec![(&selector, false)], &mut HashSet::new());
    dependencies
}

#[test]
fn unrelated_has_membership_does_not_widen_proven_text_and_graphics_updates() {
    let text = MutationKind::TextChildren.impact();
    let path = MutationKind::Attribute("d").impact();
    let mixed = text
        .union(path)
        .union(MutationKind::Attribute("style").impact());
    for selector in [
        "html:has(.active)",
        "html:has([class])",
        "html:has(:checked)",
        "html:has(:nth-child(2 of .active))",
        ".a:has(+ .b .active)",
    ] {
        let deps = dependencies(selector);
        for impact in [text, path, mixed] {
            assert!(
                !deps.needs_document_refresh(impact),
                "{selector}: {impact:?}"
            );
        }
        for impact in [
            MutationKind::ChildList.impact(),
            MutationKind::Attribute("class").impact(),
            MutationKind::State.impact(),
            mixed.union(MutationKind::Attribute("data-active").impact()),
        ] {
            assert!(
                deps.needs_document_refresh(impact),
                "{selector}: {impact:?}"
            );
        }
    }
}

#[test]
fn text_empty_direction_validity_and_slots_keep_nonlocal_dependencies() {
    let text = MutationKind::TextChildren.impact();
    for selector in [
        "html:has(:empty)",
        "html:has(:is(:empty))",
        "html:has(:not(:empty))",
        "html:has(:nth-child(1 of :empty))",
        "html:has(:dir(rtl))",
        "body:dir(rtl)",
        "html:has(:valid)",
        "html:has(:invalid)",
        "html:has(:in-range)",
        "html:has(:out-of-range)",
        "slot:has-slotted",
    ] {
        assert!(
            dependencies(selector).needs_document_refresh(text),
            "{selector}"
        );
    }
}

#[test]
fn ordinary_validity_rules_retain_bounded_form_aggregation_roots() {
    for selector in [
        "form:valid",
        "fieldset:invalid",
        ":is(form:invalid, fieldset:valid)",
    ] {
        assert!(
            !dependencies(selector).needs_document_refresh(MutationKind::TextChildren.impact())
        );
    }
}

#[test]
fn relative_graphics_attributes_and_mixed_declarations_widen_when_relevant() {
    let text = MutationKind::TextChildren.impact();
    let graphics = MutationKind::Attribute("d").impact();
    for selector in [
        "html:has([d])",
        "html:has(:not([points]))",
        "html:has(:is([transform]))",
        "html:has(:nth-child(1 of [X]))",
        ".a:has(+ svg [cx])",
    ] {
        let deps = dependencies(selector);
        assert!(deps.needs_document_refresh(graphics), "{selector}");
        assert!(
            deps.needs_document_refresh(graphics.union(text)),
            "{selector}"
        );
        assert!(!deps.needs_document_refresh(text), "{selector}");
    }
    assert!(
        dependencies("html:has([style])").needs_document_refresh(
            graphics
                .union(text)
                .union(MutationKind::Attribute("style").impact())
        )
    );
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

#[test]
fn relational_selectors_widen_resource_attribute_writes_only_when_they_read_them() {
    let src = MutationKind::Attribute("src").impact();
    assert!(src.is_resource_attribute_only());
    for selector in ["main:has(.active)", "main:has(img)", "main:has([class])"] {
        assert!(
            !dependencies(selector).needs_document_refresh(src),
            "{selector}"
        );
    }
    for selector in [
        "main:has([src])",
        "main:has(:is([width]))",
        "main:has(:not([sizes]))",
    ] {
        assert!(
            dependencies(selector).needs_document_refresh(src),
            "{selector}"
        );
    }
    assert!(
        dependencies("main:has(.active)")
            .needs_document_refresh(src.union(MutationKind::Attribute("class").impact()))
    );
}
