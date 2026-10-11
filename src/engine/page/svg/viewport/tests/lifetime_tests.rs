use super::*;

#[test]
fn compositing_changes_reuse_pixels_but_viewport_and_density_changes_do_not() {
    let mut page = live(
        "<svg width=100 height=50 viewBox='0 0 100 50' style='display:block;width:80px;height:40px'><rect width=100 height=50 fill=red /></svg>",
        1.,
    );
    layout(&mut page);
    let key = inline_svg_key(&svg(&page));
    let original = image(&page).bgra.clone();
    page.acknowledge_image_updates(std::slice::from_ref(&key));
    svg(&page).set_animation_style("opacity:.5;transform:scale(.75);clip-path:inset(0 25% 0 0)");
    layout(&mut page);
    assert!(Arc::ptr_eq(&original, &image(&page).bgra));
    assert!(!page.has_image_update(&key));
    svg(&page).set_attr("style", "display:block;width:60px;height:30px");
    layout(&mut page);
    assert_eq!((image(&page).width, image(&page).height), (60, 30));
    assert!(!Arc::ptr_eq(&original, &image(&page).bgra));
    let resized = image(&page).bgra.clone();
    page.set_media_environment(MediaEnvironment::new(800., 600., 1.25, false));
    layout(&mut page);
    assert_eq!((image(&page).width, image(&page).height), (75, 38));
    assert!(!Arc::ptr_eq(&resized, &image(&page).bgra));
    assert_eq!(page.layout_image_dimensions(&key), Some((100, 50)));
}

#[test]
fn source_changes_invalidate_published_pixels_and_natural_size_independently() {
    let mut page = live(
        "<svg width=100 height=50 style='display:block;width:40px;height:20px'><rect width='100%' height='100%' fill=red /></svg>",
        1.,
    );
    layout(&mut page);
    let old = image(&page).bgra.clone();
    let key = inline_svg_key(&svg(&page));
    svg(&page).set_attr("width", "200");
    page.dom
        .elements_named("rect")
        .next()
        .unwrap()
        .set_attr("fill", "blue");
    layout(&mut page);
    assert_eq!(page.layout_image_dimensions(&key), Some((200, 50)));
    assert_eq!((image(&page).width, image(&page).height), (40, 20));
    assert!(!Arc::ptr_eq(&old, &image(&page).bgra));
    assert_eq!(pixel(image(&page), 20, 10), [255, 0, 0, 255]);
}

#[test]
fn hidden_and_removed_svg_caches_have_dom_owned_lifetimes() {
    let mut page = live(
        "<section><svg width=100 height=50 style='display:block;width:40px;height:20px'><rect width='100%' height='100%' fill=red /></svg></section>",
        1.,
    );
    layout(&mut page);
    let node = svg(&page);
    let key = inline_svg_key(&node);
    let original = image(&page).bgra.clone();
    let section = page.dom.elements_named("section").next().unwrap();
    section.set_attr("style", "display:none");
    node.set_attr("style", "display:block;width:60px;height:30px");
    layout(&mut page);
    assert!(Arc::ptr_eq(&original, &image(&page).bgra));
    section.remove_attr("style");
    layout(&mut page);
    assert_eq!((image(&page).width, image(&page).height), (60, 30));
    assert!(Node::remove_child(&section, &node));
    layout(&mut page);
    assert!(!page.images.contains_key(&key));
    assert!(!page.inline_svgs.entries.contains_key(&key));
    assert!(!page.inline_svgs.intrinsic.contains_key(&key));
    assert!(!page.inline_svgs.versions.contains_key(&node.id()));
}

#[test]
fn optional_source_budget_declines_to_the_existing_natural_decoder() {
    let mut page = live(
        "<svg width=4 height=2><rect width=4 height=2 fill=red /></svg>",
        1.,
    );
    let node = svg(&page);
    let input = InlineSvgInput::new(&node, None);
    assert!(page.inline_svgs.can_admit("incoming", &input));
    let oversized = Entry {
        node: node.id(),
        source: Arc::from(" ".repeat(MAX_RETAINED_SOURCE_BYTES)),
        fonts: Arc::default(),
        published: None,
    };
    page.inline_svgs
        .entries
        .insert("existing".into(), oversized);
    assert!(!page.inline_svgs.can_admit("incoming", &input));
    assert!(
        page.inline_svgs.can_admit("existing", &input),
        "replacement excludes the old entry"
    );
    layout(&mut page);
    assert_eq!((image(&page).width, image(&page).height), (4, 2));
}

#[test]
fn malformed_source_cannot_keep_stale_valid_pixels() {
    let mut page = live(
        "<svg width=4 height=2><rect width=4 height=2 fill=red /></svg>",
        1.,
    );
    layout(&mut page);
    let node = svg(&page);
    let key = inline_svg_key(&node);
    node.set_attr("viewBox", "0 0 0 0");
    node.set_attr("width", "0");
    node.set_attr("height", "0");
    layout(&mut page);
    assert!(!page.images.contains_key(&key));
    assert!(!page.inline_svgs.entries.contains_key(&key));
    assert!(
        page.diagnostics
            .iter()
            .any(|message| message.starts_with("inline SVG "))
    );
}

#[test]
fn color_only_refresh_rasterizes_against_the_retained_current_viewport() {
    use crate::engine::invalidation::{InvalidationImpact, RenderInvalidation};
    let mut page = live(
        "<style>svg{color:red}svg:hover{color:blue}</style><svg width=40 height=20 style='display:block;width:20px;height:10px'><rect width=40 height=20 fill=currentColor /></svg>",
        2.,
    );
    let output = layout(&mut page);
    let node = svg(&page);
    let key = inline_svg_key(&node);
    page.acknowledge_image_updates(std::slice::from_ref(&key));
    assert_eq!(pixel(image(&page), 20, 10), [0, 0, 255, 255]);
    Node::update_hover_path(&mut Vec::new(), Some(node.clone()));
    page.refresh_layout_styles_after_invalidation_for_viewport(
        800.,
        600.,
        &RenderInvalidation {
            roots: vec![node.id()],
            impact: InvalidationImpact::STYLE,
            ..Default::default()
        },
    );
    page.refresh_inline_svg_colors();
    page.prepare_inline_svg_images(&output);
    assert_eq!((image(&page).width, image(&page).height), (40, 20));
    assert_eq!(pixel(image(&page), 20, 10), [255, 0, 0, 255]);
    assert!(page.has_image_update(&key));
}

#[test]
fn invalid_new_raster_request_cannot_reuse_an_unpublished_old_source() {
    let mut page = live(
        "<svg width=4 height=2><rect width=4 height=2 fill=red /></svg>",
        1.,
    );
    layout(&mut page);
    let node = svg(&page);
    let key = inline_svg_key(&node);
    page.dom
        .elements_named("rect")
        .next()
        .unwrap()
        .set_attr("fill", "blue");
    page.refresh_resources_for_viewport(800., 600.);
    assert_eq!(page.inline_svgs.entries[&key].published, None);
    let mut output = crate::engine::layout_page(&page, 800., 600., &mut Measurer);
    for item in &mut output.items {
        if let DisplayItem::Image { rect, url, .. } = item
            && url == &key
        {
            rect.width = f32::MAX;
        }
    }
    page.prepare_inline_svg_images(&output);
    assert!(!page.images.contains_key(&key));
    assert!(
        page.diagnostics
            .iter()
            .any(|message| message.contains("dimension budget"))
    );
}
