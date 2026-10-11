use super::*;

#[test]
fn ambiguous_containing_blocks_and_effect_owners_are_not_retained() {
    for css in [
        "body{transform:translate(2px,3px)}",
        "body{position:relative}",
        "body{filter:blur(2px)}",
        "body{perspective:200px}",
        "body{contain:paint}",
        "body{will-change:transform}",
        "body{zoom:1.25}",
        "body{opacity:.9}",
        "body{clip-path:inset(10px)}",
        "body{mask-image:linear-gradient(black,transparent)}",
        "#overlay{inset:auto}",
        "#overlay{top:auto;bottom:auto}",
    ] {
        let page = page(css);
        let output = layout_page(&page, 800., 600., &mut FixedMeasurer);
        assert!(output.fixed_layers.layers.is_empty(), "{css}");
    }
}

#[test]
fn style_attribute_dependencies_disable_the_ownership_proof() {
    for css in [
        "body:has([style]) aside{height:900px}",
        "[style] + aside{position:fixed;inset:0}",
        "@scope(body){#overlay{color:red}}",
    ] {
        let page = page(css);
        assert!(
            layout_page(&page, 800., 600., &mut FixedMeasurer)
                .fixed_layers
                .layers
                .is_empty()
        );
    }
}

#[test]
fn unknown_mixed_and_viewport_work_requires_full_layout() {
    let mut page = page("");
    let mut retained = layout_page(&page, 800., 600., &mut FixedMeasurer);
    let valid = refresh(&mut page, "bar", "transform:scaleX(.7)");
    let outside = page.dom.elements_named("main").next().unwrap();
    for invalidation in [
        RenderInvalidation {
            roots: vec![outside.id()],
            ..valid.clone()
        },
        RenderInvalidation {
            roots: vec![outside.id(), node(&page, "bar").id()],
            ..valid.clone()
        },
        RenderInvalidation {
            impact: valid.impact.union(MutationKind::ChildList.impact()),
            ..valid.clone()
        },
        RenderInvalidation {
            rebuild_style_rules: true,
            ..valid.clone()
        },
        RenderInvalidation {
            removed_nodes: vec![outside.id()],
            ..valid.clone()
        },
        RenderInvalidation {
            roots: Vec::new(),
            ..valid.clone()
        },
    ] {
        let before = retained.clone();
        assert!(!retained.try_refresh_fixed_layers(
            &page,
            [800., 600., 800.],
            &invalidation,
            &mut FixedMeasurer
        ));
        assert_same(&retained, &before, &page);
    }
    assert!(!retained.try_refresh_fixed_layers(
        &page,
        [780., 600., 800.],
        &valid,
        &mut FixedMeasurer
    ));
}

#[test]
fn bounds_on_optional_ownership_do_not_drop_content() {
    let mut page = page("");
    let overlay = node(&page, "overlay");
    let hidden = Node::create_element("div");
    hidden.set_attr("style", "display:none");
    Node::append_child(&overlay, hidden.clone());
    for _ in 0..MAX_MEMBERS {
        let child = Node::create_element("div");
        Node::append_child(&hidden, child);
    }
    page.refresh_resources_for_viewport(800., 600.);
    let output = layout_page(&page, 800., 600., &mut FixedMeasurer);
    assert!(output.fixed_layers.layers.is_empty());
    assert!(output.node_bounds.contains_key(&node(&page, "title").id()));
}
