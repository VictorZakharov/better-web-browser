use super::*;

#[test]
fn full_rebuild_evicts_unassigned_and_detached_styles_without_a_removal_log() {
    let dom = dom::parse(
        "<style>p{color:red}p::before{content:'old'}</style><x-host><p id=light>light</p></x-host><p id=detached>removed</p>",
    );
    let host = dom.elements_named("x-host").next().unwrap();
    let light = dom.elements_named("p").next().unwrap();
    let detached = dom.elements_named("p").nth(1).unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    let old_generated = [&light, &detached]
        .into_iter()
        .flat_map(|node| {
            Node::descendants(
                &styles
                    .generated_pseudo(node, PseudoElement::Before)
                    .unwrap(),
            )
        })
        .map(|node| node.id())
        .collect::<Vec<_>>();
    Node::attach_shadow(
        &host,
        crate::engine::dom::ShadowRootMode::Open,
        false,
        false,
        false,
    )
    .unwrap();
    Node::remove_from_parent(&detached);
    Node::set_text_content(
        &dom.elements_named("style").next().unwrap(),
        "p{color:blue}p::before{content:'new'}",
    );
    let stats = styles.rebuild_rules_for_media_environment(
        &dom,
        "",
        &[],
        MediaEnvironment::new(800.0, 800.0, 1.0, false),
        &[],
    );
    assert!(stats.removed_styles >= 4);
    let fresh = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(styles.styles, fresh.styles);
    for node in [&light, &detached] {
        assert!(!styles.styles.contains_key(&node.id()));
        assert!(
            !styles
                .pseudo_styles
                .keys()
                .any(|(origin, _)| *origin == node.id())
        );
        assert!(
            styles
                .generated_pseudo(node, PseudoElement::Before)
                .is_none()
        );
    }
    assert!(
        old_generated
            .iter()
            .all(|node| !styles.generated_styles.contains_key(node))
    );
    assert_eq!(
        styles.computed_style_for_node(&light).unwrap().color,
        Color::rgb(0, 0, 255)
    );
    assert_eq!(
        styles
            .generated_pseudo(&light, PseudoElement::Before)
            .unwrap()
            .text_content(),
        "new"
    );
}

#[test]
fn viewport_changes_require_layout_even_when_computed_styles_are_equal() {
    let dom = dom::parse("<main><p>auto width follows the viewport</p></main>");
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    let previous = styles.styles.clone();
    for (width, height) in [(900.0, 800.0), (900.0, 900.0)] {
        let stats = styles.rebuild_rules_for_media_environment(
            &dom,
            "",
            &[],
            MediaEnvironment::new(width, height, 1.0, false),
            &[],
        );
        assert_eq!(styles.styles, previous);
        assert_eq!(stats.changed_styles, 0);
        assert!(stats.layout_changed);
    }
    let unchanged = styles.rebuild_rules_for_media_environment(
        &dom,
        "",
        &[],
        MediaEnvironment::new(900.0, 900.0, 1.0, false),
        &[],
    );
    assert!(!unchanged.layout_changed);
}
