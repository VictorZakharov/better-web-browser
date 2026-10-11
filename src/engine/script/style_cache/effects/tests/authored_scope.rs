use super::*;

#[test]
fn authored_declarations_use_target_scope_only_after_stylesheet_proof() {
    for (css, narrow) in [
        ("main{color:red}main + aside{color:blue}", true),
        ("[style] + aside{color:blue}", false),
        ("body:has(main[style]) aside{color:blue}", false),
        ("@scope(main){span{color:blue}}", false),
    ] {
        let (dom, mut state) = state(&format!(
            "<style>{css}</style><main><span></span></main><aside></aside>"
        ));
        let target = dom.elements_named("main").next().unwrap();
        let parent = target.parent().unwrap();
        assert!(
            !state.inline_style_uses_target_scope(),
            "unknown rules must not authorize narrow roots"
        );
        color(&mut state, &target);
        target.set_attr("style", "color:green");
        let revision = state.css_animation_revision;
        state.record_mutation(Some(&target), MutationKind::Attribute("style"));
        assert_ne!(
            state.css_animation_revision, revision,
            "author changes still rediscover animations"
        );
        let expected = if narrow { target.id() } else { parent.id() };
        assert_eq!(
            state.pending_invalidation.snapshot(1).roots,
            vec![expected],
            "{css}"
        );
        assert_eq!(
            state.pending_layout_invalidation.snapshot(1).roots,
            vec![expected],
            "{css}"
        );
        assert_eq!(color(&mut state, &target), "rgb(0, 128, 0)");
    }
}

#[test]
fn replacing_rules_discards_the_old_selector_scope_proof() {
    let (dom, mut state) = state("<main></main><aside></aside>");
    let target = dom.elements_named("main").next().unwrap();
    color(&mut state, &target);
    assert!(state.inline_style_uses_target_scope());
    state.replace_document_stylesheets(&[crate::engine::css::StylesheetSource::injected(
        "https://example.com/late.css",
        "[style] + aside{color:blue}".into(),
    )]);
    assert!(!state.inline_style_uses_target_scope());
    target.set_attr("style", "opacity:.5");
    state.record_mutation(Some(&target), MutationKind::Attribute("style"));
    assert!(
        state
            .pending_invalidation
            .snapshot(1)
            .roots
            .contains(&target.parent().unwrap().id())
    );
    let sibling = dom.elements_named("aside").next().unwrap();
    assert_eq!(color(&mut state, &sibling), "rgb(0, 0, 255)");
}
