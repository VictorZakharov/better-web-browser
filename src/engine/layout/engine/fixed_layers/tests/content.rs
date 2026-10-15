use super::*;

fn add_counter(page: &mut Page) -> NodeRef {
    let counter = Node::create_element("span");
    counter.set_attr("id", "counter");
    Node::set_text_content(&counter, "initial");
    Node::append_child(&node(page, "overlay"), counter.clone());
    page.refresh_resources_for_viewport(800., 600.);
    counter
}

fn text_change(page: &mut Page, target: &NodeRef, text: &str) -> RenderInvalidation {
    let removed_nodes = target
        .children
        .borrow()
        .iter()
        .flat_map(Node::descendants)
        .map(|node| node.id())
        .collect();
    Node::set_text_content(target, text);
    let invalidation = RenderInvalidation {
        roots: vec![target.parent().unwrap().id()],
        impact: MutationKind::TextChildren.impact(),
        mutation_count: 1,
        removed_nodes,
        removals_are_local: true,
        ..Default::default()
    };
    page.refresh_resources_after_invalidation_for_viewport(800., 600., &invalidation);
    invalidation
}

fn reflow(output: &mut LayoutOutput, page: &Page, dirty: &RenderInvalidation) -> bool {
    output.try_refresh_fixed_layers(page, [800., 600., 800.], dirty, &mut FixedMeasurer)
}

#[test]
fn changing_fixed_text_replaces_owned_fragments_and_matches_full_layout() {
    let mut page = page("");
    let counter = add_counter(&mut page);
    let mut retained = layout_page(&page, 800., 600., &mut FixedMeasurer);
    let background = page.dom.elements_named("main").next().unwrap();
    let background_geometry = retained.fragments.element(background.id());
    for text in [
        "123 FPS",
        "longer text with wrapping and spaces",
        "",
        "雪 ☃ 48 FPS",
    ] {
        let invalidation = text_change(&mut page, &counter, text);
        assert!(reflow(&mut retained, &page, &invalidation), "{text}");
        assert_same(
            &retained,
            &layout_page(&page, 800., 600., &mut FixedMeasurer),
            &page,
        );
        assert_eq!(
            retained.fragments.element(background.id()),
            background_geometry
        );
        for id in invalidation.removed_nodes {
            assert!(retained.fragments.range_text(id, 0, 100).is_empty());
        }
    }
}

#[test]
fn text_and_local_svg_graphics_can_share_one_independent_reflow() {
    let mut page = Page::parse(
        "<!doctype html><style>body{margin:0}section{position:fixed;left:10px;top:20px;width:200px}\
         svg{width:100px;height:40px}</style><main>unrelated background</main><section>\
         <span id=counter>10 FPS</span><svg xmlns='http://www.w3.org/2000/svg' width=100 height=40>\
         <path id=path d='M0 0L20 20' stroke='red'/></svg></section>",
        "https://example.test/",
    );
    page.refresh_resources_for_viewport(800., 600.);
    let mut retained = layout_page(&page, 800., 600., &mut FixedMeasurer);
    let counter = node(&page, "counter");
    let mut dirty = text_change(&mut page, &counter, "144 FPS");
    let path = node(&page, "path");
    path.set_attr("d", "M0 30L50 0L90 20");
    dirty.impact = dirty.impact.union(MutationKind::Attribute("d").impact());
    dirty.roots.push(path.id());
    page.refresh_resources_after_invalidation_for_viewport(800., 600., &dirty);
    assert!(reflow(&mut retained, &page, &dirty));
    assert_same(
        &retained,
        &layout_page(&page, 800., 600., &mut FixedMeasurer),
        &page,
    );
}

#[test]
fn relational_text_dependencies_and_unknown_removals_fall_back_atomically() {
    for css in [
        "body:has(#counter:empty) main{height:333px}",
        "body:dir(rtl) main{height:444px}",
    ] {
        let mut page = page(css);
        let counter = add_counter(&mut page);
        let mut retained = layout_page(&page, 800., 600., &mut FixedMeasurer);
        let before = retained.clone();
        let dirty = text_change(&mut page, &counter, "");
        assert!(!reflow(&mut retained, &page, &dirty), "{css}");
        assert_same(&retained, &before, &page);
    }
    let mut page = page("");
    let counter = add_counter(&mut page);
    let mut retained = layout_page(&page, 800., 600., &mut FixedMeasurer);
    let before = retained.clone();
    let valid = text_change(&mut page, &counter, "changed");
    for dirty in [
        RenderInvalidation {
            removals_are_local: false,
            ..valid.clone()
        },
        RenderInvalidation {
            removed_nodes: vec![page.dom.document.id()],
            ..valid.clone()
        },
        RenderInvalidation {
            impact: valid.impact.union(MutationKind::ChildList.impact()),
            ..valid.clone()
        },
    ] {
        assert!(!reflow(&mut retained, &page, &dirty));
        assert_same(&retained, &before, &page);
    }
}

#[test]
fn text_evidence_does_not_authorize_changed_element_membership() {
    let mut page = page("");
    let counter = add_counter(&mut page);
    let mut retained = layout_page(&page, 800., 600., &mut FixedMeasurer);
    let before = retained.clone();
    let dirty = text_change(&mut page, &counter, "changed");
    Node::append_child(&counter, Node::create_element("b"));
    page.refresh_resources_for_viewport(800., 600.);
    assert!(!reflow(&mut retained, &page, &dirty));
    assert_same(&retained, &before, &page);
}
