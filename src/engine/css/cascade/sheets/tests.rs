use super::*;

#[test]
fn shares_only_identical_rule_inputs_and_never_computed_styles() {
    let dom = dom::parse("<style>.a {color:red}</style><p class=a>x</p>");
    let first = StyleSet::from_dom(&dom, &[], 800.0);
    let mut second = StyleSet::from_dom(&dom, &[], 800.0);
    assert!(Rc::ptr_eq(&first.compiled, &second.compiled));
    second.clear_computed_styles();
    assert!(!first.styles.is_empty());
    assert!(second.styles.is_empty());
    assert!(!Rc::ptr_eq(
        &first.compiled,
        &StyleSet::from_dom(&dom, &[], 900.0).compiled
    ));
    let other = dom::parse("<style>.a {color:red}</style><p class=a>x</p>");
    assert!(!Rc::ptr_eq(
        &first.compiled,
        &StyleSet::from_dom(&other, &[], 800.0).compiled
    ));
    Node::set_text_content(
        &dom.elements_named("style").next().unwrap(),
        ".a {color:blue}",
    );
    let updated = StyleSet::from_dom(&dom, &[], 800.0);
    assert!(!Rc::ptr_eq(&first.compiled, &updated.compiled));
    assert_eq!(
        updated.get(&dom.elements_named("p").next().unwrap()).color,
        Color::rgb(0, 0, 255)
    );
}

#[test]
fn rebuilding_rules_reconciles_values_and_generated_boxes_against_a_fresh_cascade() {
    let dom = dom::parse(
        "<style>:root{--tone:red} p{color:var(--tone);width:10vw} p::before{content:'old'}</style><p>x</p>",
    );
    let target = dom.elements_named("p").next().unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    let variables = styles.get(&target).custom_properties.clone();
    let before = styles
        .generated_pseudo(&target, PseudoElement::Before)
        .unwrap();
    let environment = MediaEnvironment::new(800.0, 800.0, 1.0, false);
    let unchanged = styles.rebuild_rules_for_media_environment(&dom, "", &[], environment, &[]);
    assert!(!unchanged.layout_changed);
    assert!(std::sync::Arc::ptr_eq(
        &variables,
        &styles.get(&target).custom_properties
    ));
    assert_eq!(
        styles
            .generated_pseudo(&target, PseudoElement::Before)
            .unwrap()
            .id(),
        before.id()
    );
    Node::set_text_content(
        &dom.elements_named("style").next().unwrap(),
        ":root{--tone:blue} p{color:var(--tone);width:20vw} p::after{content:'new'}",
    );
    let changed = styles.rebuild_rules_for_media_environment(&dom, "", &[], environment, &[]);
    assert!(changed.layout_changed);
    let fresh = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(styles.styles, fresh.styles);
    assert!(
        styles
            .generated_pseudo(&target, PseudoElement::Before)
            .is_none()
    );
    assert_eq!(
        styles
            .generated_pseudo(&target, PseudoElement::After)
            .unwrap()
            .text_content(),
        "new"
    );
}

#[test]
fn shadow_scopes_adopted_sources_and_media_invalidate_sharing() {
    let dom = dom::parse("<x-one></x-one><x-two></x-two>");
    let roots = ["x-one", "x-two"].map(|tag| {
        Node::attach_shadow(
            &dom.elements_named(tag).next().unwrap(),
            crate::engine::dom::ShadowRootMode::Open,
            false,
            false,
            false,
        )
        .unwrap()
    });
    let sheet = |source: &str, media: &str| crate::engine::AdoptedStyleSheet {
        source: source.into(),
        media: media.into(),
        base_url: "https://example.test/component.css".into(),
    };
    roots[0].set_adopted_stylesheets(vec![sheet(":host{color:red}", "")]);
    let first = StyleSet::from_dom(&dom, &[], 800.0);
    assert!(Rc::ptr_eq(
        &first.compiled,
        &StyleSet::from_dom(&dom, &[], 800.0).compiled
    ));
    roots[0].set_adopted_stylesheets(vec![]);
    roots[1].set_adopted_stylesheets(vec![sheet(":host{color:red}", "")]);
    let moved = StyleSet::from_dom(&dom, &[], 800.0);
    assert!(!Rc::ptr_eq(&first.compiled, &moved.compiled));
    assert_eq!(
        moved
            .get(&dom.elements_named("x-two").next().unwrap())
            .color,
        Color::rgb(255, 0, 0)
    );
    roots[1].set_adopted_stylesheets(vec![sheet(":host{color:blue}", "")]);
    let edited = StyleSet::from_dom(&dom, &[], 800.0);
    assert!(!Rc::ptr_eq(&moved.compiled, &edited.compiled));
    assert_eq!(
        edited
            .get(&dom.elements_named("x-two").next().unwrap())
            .color,
        Color::rgb(0, 0, 255)
    );
    roots[1].set_adopted_stylesheets(vec![sheet(":host{color:blue}", "(min-width:900px)")]);
    let hidden = StyleSet::from_dom(&dom, &[], 800.0);
    assert!(!Rc::ptr_eq(&edited.compiled, &hidden.compiled));
    assert!(hidden.compiled.rules.is_empty());
    let visible = StyleSet::from_dom(&dom, &[], 1000.0);
    assert!(!visible.compiled.rules.is_empty());
    let weak = Rc::downgrade(&visible.compiled);
    drop(visible);
    assert!(
        weak.upgrade().is_none(),
        "the global lookup must not retain compiled sources"
    );
}

#[test]
fn source_urls_and_source_order_are_part_of_compiled_rule_identity() {
    let dom = dom::parse("<p>x</p>");
    let environment = MediaEnvironment::new(800.0, 600.0, 1.0, false);
    let first = collect(
        &dom.document,
        "",
        &[
            StylesheetSource::injected("https://example.com/a.css", "p{color:red}".into()),
            StylesheetSource::injected("https://example.com/b.css", "p{color:blue}".into()),
        ],
        environment,
    );
    let swapped = collect(
        &dom.document,
        "",
        &[
            StylesheetSource::injected("https://example.com/b.css", "p{color:blue}".into()),
            StylesheetSource::injected("https://example.com/a.css", "p{color:red}".into()),
        ],
        environment,
    );
    assert!(!Rc::ptr_eq(&first, &swapped));
    let base_changed = collect(
        &dom.document,
        "",
        &[
            StylesheetSource::injected("https://other.example/a.css", "p{color:red}".into()),
            StylesheetSource::injected("https://example.com/b.css", "p{color:blue}".into()),
        ],
        environment,
    );
    assert!(!Rc::ptr_eq(&first, &base_changed));
}
