use super::*;

fn environment() -> MediaEnvironment {
    MediaEnvironment::new(800.0, 800.0, 1.0, false)
}

fn dirty(root: &NodeRef) -> RenderInvalidation {
    RenderInvalidation::full(root.id())
}

#[test]
fn identical_rule_inputs_refresh_only_dirty_dom_and_keep_generated_content_live() {
    let dom = dom::parse(&format!(
        "<style>section{{color:red}} .blue{{color:blue}} b::before{{content:attr(data-label)}}</style>\
         <section><b data-label=old>text</b></section><aside>{}</aside>",
        "<p>unaffected</p>".repeat(500)
    ));
    let root = dom.elements_named("section").next().unwrap();
    let child = dom.elements_named("b").next().unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    root.set_attr("class", "blue");
    child.set_attr("data-label", "new");
    let stats =
        styles.refresh_rules_after_invalidation(&dom, "", &[], environment(), &dirty(&root));
    assert!(!stats.full_rebuild);
    assert_eq!(stats.recomputed_styles, 3);
    assert_eq!(styles.styles, StyleSet::from_dom(&dom, &[], 800.0).styles);
    assert_eq!(
        styles
            .generated_pseudo(&child, PseudoElement::Before)
            .unwrap()
            .text_content(),
        "new"
    );
}

#[test]
fn changed_keyless_rules_widen_a_small_dirty_root_to_the_document() {
    let dom = dom::parse(
        "<style>:not(b){color:red}</style><section><b>x</b></section><aside><p>affected</p></aside>",
    );
    let root = dom.elements_named("section").next().unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    Node::set_text_content(
        &dom.elements_named("style").next().unwrap(),
        ":not(b){color:blue}",
    );
    let stats =
        styles.refresh_rules_after_invalidation(&dom, "", &[], environment(), &dirty(&root));
    assert!(stats.full_rebuild);
    assert_eq!(styles.styles, StyleSet::from_dom(&dom, &[], 800.0).styles);
    assert_eq!(
        styles.get(&dom.elements_named("p").next().unwrap()).color,
        Color::rgb(0, 0, 255)
    );
}

#[test]
fn unchanged_rules_still_prune_newly_unassigned_and_removed_nodes() {
    let dom = dom::parse(
        "<style>p::before{content:'x'}</style><main><x-host><p>light</p></x-host><p>removed</p></main>",
    );
    let root = dom.elements_named("main").next().unwrap();
    let host = dom.elements_named("x-host").next().unwrap();
    let removed = dom.elements_named("p").nth(1).unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    Node::attach_shadow(
        &host,
        crate::engine::dom::ShadowRootMode::Open,
        false,
        false,
        false,
    )
    .unwrap();
    Node::remove_from_parent(&removed);
    let stats =
        styles.refresh_rules_after_invalidation(&dom, "", &[], environment(), &dirty(&root));
    assert!(!stats.full_rebuild);
    assert!(stats.removed_styles >= 4);
    assert_eq!(styles.styles, StyleSet::from_dom(&dom, &[], 800.0).styles);
    assert!(styles.generated_nodes.is_empty());
    assert!(styles.generated_styles.is_empty());
}

#[test]
fn viewport_and_inline_url_base_changes_cannot_take_the_scoped_path() {
    let dom = dom::parse(
        "<section>x</section><p style='background-image:url(image.png);width:10vw'>elsewhere</p>",
    );
    let root = dom.elements_named("section").next().unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    let mut changed = environment();
    changed.viewport_width = 1000.0;
    let stats = styles.refresh_rules_after_invalidation(&dom, "", &[], changed, &dirty(&root));
    assert!(stats.full_rebuild && stats.layout_changed);
    let stats = styles.refresh_rules_after_invalidation(
        &dom,
        "https://example.test/new/",
        &[],
        changed,
        &dirty(&root),
    );
    assert!(stats.full_rebuild);
    assert_eq!(
        styles
            .get(&dom.elements_named("p").next().unwrap())
            .background_image
            .as_deref(),
        Some("https://example.test/new/image.png")
    );
}

#[test]
fn a_rule_change_with_a_body_insertion_restyles_only_the_insertion_and_rule_subjects() {
    // Wikipedia inserts module styles and body content in the same checkpoint; the body
    // child-list root used to restyle the whole document.
    let dom = dom::parse(&format!(
        "<head><style>.a{{color:red}}</style></head><body>{}</body>",
        "<p>unaffected</p>".repeat(200)
    ));
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    let style = dom.elements_named("style").next().unwrap();
    Node::set_text_content(&style, ".a{color:red} .b{color:blue}");
    let body = dom.elements_named("body").next().unwrap();
    let inserted = Node::create_element_for(&dom.document, "div");
    inserted.set_attr("class", "b");
    Node::append_child(&body, inserted);
    let invalidation = RenderInvalidation {
        roots: vec![style.id(), body.id()],
        child_list_roots: vec![body.id()],
        element_roots: Vec::new(),
        impact: crate::engine::invalidation::MutationKind::ChildList.impact(),
        mutation_count: 2,
        rebuild_style_rules: true,
        ..Default::default()
    };
    let stats =
        styles.refresh_rules_after_invalidation(&dom, "", &[], environment(), &invalidation);
    assert!(!stats.full_rebuild);
    assert!(stats.recomputed_styles < 20, "{stats:?}");
    assert_eq!(styles.styles, StyleSet::from_dom(&dom, &[], 800.0).styles);
}
