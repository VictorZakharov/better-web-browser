use super::*;

#[test]
fn multiple_independent_fixed_roots_preserve_stacking_and_sibling_geometry() {
    let mut page = page(
        "#second{position:fixed;left:35px;top:45px;width:100px;height:80px;z-index:30;background:blue}",
    );
    let root = page.dom.elements_named("body").next().unwrap();
    let second = Node::create_element("div");
    second.set_attr("id", "second");
    Node::append_child(&root, second.clone());
    page.refresh_resources_for_viewport(800., 600.);
    let mut retained = layout_page(&page, 800., 600., &mut FixedMeasurer);
    assert_eq!(retained.fixed_layers.layers.len(), 2);
    let mut invalidation = refresh(&mut page, "bar", "transform:scaleX(.8)");
    second.set_attr("style", "transform:translate(80px,90px);width:140px");
    invalidation.roots.push(second.id());
    page.refresh_layout_styles_after_invalidation_for_viewport(800., 600., &invalidation);
    assert!(retained.try_refresh_fixed_layers(
        &page,
        [800., 600., 800.],
        &invalidation,
        &mut FixedMeasurer
    ));
    assert_same(
        &retained,
        &layout_page(&page, 800., 600., &mut FixedMeasurer),
        &page,
    );
}

#[test]
fn failure_in_one_fixed_owner_does_not_partially_commit_other_changes() {
    let mut page = page("aside{position:fixed;inset:10px;width:80px;height:90px}");
    let second = page.dom.elements_named("aside").next().unwrap();
    let mut retained = layout_page(&page, 800., 600., &mut FixedMeasurer);
    let before = retained.clone();
    let mut invalidation = refresh(&mut page, "bar", "transform:scaleX(.8)");
    second.set_attr("style", "position:relative");
    invalidation.roots.push(second.id());
    page.refresh_layout_styles_after_invalidation_for_viewport(800., 600., &invalidation);
    assert!(!retained.try_refresh_fixed_layers(
        &page,
        [800., 600., 800.],
        &invalidation,
        &mut FixedMeasurer
    ));
    assert_same(&retained, &before, &page);
}

#[test]
fn generated_and_nested_fixed_paint_remain_in_the_outer_atomic_range() {
    let mut page = page(
        "#title::after{content:'after';position:absolute;right:0;bottom:0} #bar{position:fixed;left:10px;top:20px}",
    );
    let mut retained = layout_page(&page, 800., 600., &mut FixedMeasurer);
    assert_eq!(retained.fixed_layers.layers.len(), 1);
    let invalidation = refresh(
        &mut page,
        "title",
        "transform:translate(20px,30px);width:120px",
    );
    assert!(retained.try_refresh_fixed_layers(
        &page,
        [800., 600., 800.],
        &invalidation,
        &mut FixedMeasurer
    ));
    assert_same(
        &retained,
        &layout_page(&page, 800., 600., &mut FixedMeasurer),
        &page,
    );
}
