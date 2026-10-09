use super::*;

fn styles(document: &NodeRef) -> StyleSet {
    StyleSet::for_computed_style_for_media_environment(
        document,
        "https://example.test/",
        &[],
        MediaEnvironment::new(800.0, 600.0, 1.0, false),
    )
}

#[test]
fn descendant_universal_does_not_admit_unrelated_elements() {
    let dom = dom::parse(
        "<style>.dialog *{animation:none} .running .item{animation:fade 1s}</style><main><p class=item></p><section class=dialog><i></i></section><aside class=running><b class=item></b></aside></main>",
    );
    let styles = styles(&dom.document);
    for tag in ["main", "p", "section", "aside"] {
        assert!(
            !styles.may_have_css_animation(&dom.elements_named(tag).next().unwrap()),
            "{tag}"
        );
    }
    for tag in ["i", "b"] {
        assert!(
            styles.may_have_css_animation(&dom.elements_named(tag).next().unwrap()),
            "{tag}"
        );
    }
    // Mutation invalidates ancestor rejection even if the rule inputs remain identical.
    dom.elements_named("main")
        .next()
        .unwrap()
        .set_attr("class", "dialog");
    assert!(styles.may_have_css_animation(&dom.elements_named("p").next().unwrap()));
    dom.elements_named("main")
        .next()
        .unwrap()
        .set_attr("class", "");
    assert!(!styles.may_have_css_animation(&dom.elements_named("p").next().unwrap()));
}

#[test]
fn functional_attribute_and_sibling_conditions_use_the_full_matcher() {
    let dom = dom::parse(
        "<style>[data-state=on] :is(p,b){animation:fade 1s} .before + p{animation:slide 2s} main:has(> .trigger) > b{animation:move 1s}</style><main data-state=off><i></i><p></p><b></b></main>",
    );
    let styles = styles(&dom.document);
    let main = dom.elements_named("main").next().unwrap();
    let before = dom.elements_named("i").next().unwrap();
    let p = dom.elements_named("p").next().unwrap();
    let b = dom.elements_named("b").next().unwrap();
    assert!(!styles.may_have_css_animation(&p));
    assert!(!styles.may_have_css_animation(&b));
    main.set_attr("data-state", "on");
    assert!(styles.may_have_css_animation(&p));
    assert!(styles.may_have_css_animation(&b));
    main.set_attr("data-state", "off");
    before.set_attr("class", "before");
    assert!(styles.may_have_css_animation(&p));
    assert!(!styles.may_have_css_animation(&b));
    before.set_attr("class", "trigger");
    assert!(!styles.may_have_css_animation(&p));
    assert!(styles.may_have_css_animation(&b));
    Node::remove_from_parent(&before);
    assert!(!styles.may_have_css_animation(&b));
}

#[test]
fn scope_limits_are_not_animation_targets() {
    let dom = dom::parse(
        "<style>@scope (.scope) to (.limit){p{animation:fade 1s}}</style><p id=outside></p><main class=scope><p id=inside></p><section class=limit><p id=blocked></p></section></main>",
    );
    let styles = styles(&dom.document);
    let nodes = dom.elements_named("p").collect::<Vec<_>>();
    assert!(!styles.may_have_css_animation(&nodes[0]));
    assert!(styles.may_have_css_animation(&nodes[1]));
    assert!(!styles.may_have_css_animation(&nodes[2]));
    dom.elements_named("section")
        .next()
        .unwrap()
        .set_attr("class", "");
    assert!(styles.may_have_css_animation(&nodes[2]));
}

#[test]
fn document_rules_do_not_leak_into_shadow_animation_discovery() {
    let dom = dom::parse("<style>*{animation:outside 1s}</style><x-host><p></p></x-host>");
    let host = dom.elements_named("x-host").next().unwrap();
    let shadow = Node::attach_shadow(
        &host,
        crate::engine::dom::ShadowRootMode::Open,
        false,
        false,
        false,
    )
    .unwrap();
    let inner = Node::create_element_for(&host, "p");
    Node::append_child(&shadow, inner.clone());
    let styles = styles(&dom.document);
    assert!(styles.may_have_css_animation(&host));
    assert!(!styles.may_have_css_animation(&inner));
}

#[test]
fn inline_and_all_declarations_remain_conservative_candidates() {
    let dom = dom::parse(
        "<style>.inherit{all:inherit} p::before{animation:fade 1s}</style><div style='animation:fade 1s'></div><i class=inherit></i><p></p>",
    );
    let styles = styles(&dom.document);
    assert!(styles.may_have_css_animation(&dom.elements_named("div").next().unwrap()));
    assert!(styles.may_have_css_animation(&dom.elements_named("i").next().unwrap()));
    assert!(!styles.may_have_css_animation(&dom.elements_named("p").next().unwrap()));
}
