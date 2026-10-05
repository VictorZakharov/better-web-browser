use super::*;

fn raster(html: &str) -> DecodedImage {
    let page = Page::parse(html, "https://example.test/");
    let node = page.dom.elements_named("svg").last().unwrap();
    decode_inline_svg(&node, None).unwrap()
}

#[test]
fn sibling_use_and_transitive_paint_dependencies_render() {
    let image = raster(
        r##"<svg width=0 height=0><defs>
        <linearGradient id=base><stop stop-color=red /><stop offset=1 stop-color=red /></linearGradient>
        <linearGradient id=paint href="#base"/>
        <g id=shape><rect width=10 height=10 fill="url('#paint')"/></g>
        </defs></svg><svg width=10 height=10><use href="#shape"/></svg>"##,
    );
    assert_eq!(&image.bgra[20..24], [0, 0, 255, 255]);
}

#[test]
fn sibling_definition_changes_invalidate_decoder_input() {
    let mut page = Page::parse(
        r##"<svg width=0 height=0><defs><rect id=shape width=10 height=10 fill=red /></defs></svg>
        <svg width=10 height=10><use href="#shape"/></svg>"##,
        "https://example.test/",
    );
    page.refresh_resources(800.0);
    let node = page.dom.elements_named("svg").last().unwrap();
    let key = inline_svg_key(&node);
    let before = page.images[&key].bgra.clone();
    page.dom
        .elements_named("rect")
        .next()
        .unwrap()
        .set_attr("fill", "blue");
    page.refresh_resources(800.0);
    assert!(!std::sync::Arc::ptr_eq(&before, &page.images[&key].bgra));
    assert_eq!(&page.images[&key].bgra[20..24], [255, 0, 0, 255]);
}

#[test]
fn duplicate_ids_use_first_tree_match_even_outside_fragment() {
    let image = raster(
        r##"<svg><defs><rect id=shape width=10 height=10 fill=red /></defs></svg>
        <svg width=10 height=10><defs><rect id=shape width=10 height=10 fill=blue /></defs><use href="#shape"/></svg>"##,
    );
    assert_eq!(&image.bgra[20..24], [0, 0, 255, 255]);
    let image = raster(
        r##"<div id=shape></div><svg><defs><rect id=shape width=10 height=10 fill=red /></defs></svg>
        <svg width=10 height=10><use href="#shape"/></svg>"##,
    );
    assert!(image.bgra.iter().all(|byte| *byte == 0));
}

#[test]
fn xlink_namespace_and_svg2_href_precedence_are_preserved() {
    let image = raster(
        r##"<svg><defs><rect id=red width=10 height=10 fill=red /><rect id=blue width=10 height=10 fill=blue /></defs></svg>
        <svg width=10 height=10><use xlink:href="#red"/></svg>"##,
    );
    assert_eq!(&image.bgra[20..24], [0, 0, 255, 255]);
    let image = raster(
        r##"<svg><defs><rect id=red width=10 height=10 fill=red /><rect id=blue width=10 height=10 fill=blue /></defs></svg>
        <svg width=10 height=10><use xlink:href="#red" href="#blue"/></svg>"##,
    );
    assert_eq!(&image.bgra[20..24], [255, 0, 0, 255]);
}

#[test]
fn sibling_use_inherits_current_color_from_instance() {
    let image = raster(
        r##"<svg color=red><defs><rect id=shape width=10 height=10 fill=currentColor /></defs></svg>
        <svg width=10 height=10 color=blue><use href="#shape"/></svg>"##,
    );
    assert_eq!(&image.bgra[20..24], [255, 0, 0, 255]);
}

#[test]
fn cyclic_missing_and_external_references_are_bounded() {
    let image = raster(
        r##"<svg><defs><g id=a><use href="#b"/></g><g id=b><use href="#a"/></g></defs></svg>
        <svg width=10 height=10><use href="#a"/><use href="#missing"/><use href="https://example.test/a.svg#shape"/></svg>"##,
    );
    assert!(image.bgra.iter().all(|byte| *byte == 0));
}

#[test]
fn serializer_rejects_escaped_expansion_and_deep_trees() {
    let page = Page::parse(
        "<svg width=10 height=10><desc></desc></svg>",
        "https://example.test/",
    );
    let desc = page.dom.elements_named("desc").next().unwrap();
    desc.set_attr("title", &"&".repeat(MAX_SVG_SOURCE_BYTES / 4));
    let node = page.dom.elements_named("svg").next().unwrap();
    assert!(
        serialize::source(&node, None)
            .unwrap_err()
            .contains("byte budget")
    );
    let html = format!(
        "<svg>{}<rect width=10 height=10/>{}</svg>",
        "<g>".repeat(260),
        "</g>".repeat(260)
    );
    let page = Page::parse(&html, "https://example.test/");
    let node = page.dom.elements_named("svg").next().unwrap();
    assert!(
        serialize::source(&node, None)
            .unwrap_err()
            .contains("structural budget")
    );
}

#[test]
fn overlapping_dependency_roots_keep_child_in_its_container() {
    let image = raster(
        r##"<svg><defs><g id=outer><rect id=inner width=5 height=10 fill=red /></g></defs></svg>
        <svg width=10 height=10><use href="#inner"/><use href="#outer" x=5 /></svg>"##,
    );
    assert_eq!(&image.bgra[8..12], [0, 0, 255, 255]);
    assert_eq!(&image.bgra[28..32], [0, 0, 255, 255]);
}

#[test]
fn shared_clip_and_inline_style_urls_affect_real_pixels() {
    let image = raster(
        r##"<svg><defs><clipPath id=half><rect width=5 height=10 /></clipPath></defs></svg>
        <svg width=10 height=10><rect width=10 height=10 fill=red style="clip-path:url('#half')" /></svg>"##,
    );
    assert_eq!(&image.bgra[8..12], [0, 0, 255, 255]);
    assert_eq!(&image.bgra[28..32], [0, 0, 0, 0]);
}

#[test]
fn shared_definitions_do_not_cross_shadow_tree_boundaries() {
    use crate::engine::dom::ShadowRootMode;
    let page = Page::parse(
        r##"<svg><defs><rect id=shape width=10 height=10 fill=red /></defs></svg>
        <div></div><svg width=10 height=10><use href="#shape"/></svg>"##,
        "https://example.test/",
    );
    let node = page.dom.elements_named("svg").last().unwrap();
    let host = page.dom.elements_named("div").next().unwrap();
    let shadow = Node::attach_shadow(&host, ShadowRootMode::Open, false, false, false).unwrap();
    Node::append_child(&shadow, node.clone());
    let image = decode_inline_svg(&node, None).unwrap();
    assert!(image.bgra.iter().all(|byte| *byte == 0));
}

#[test]
fn exponential_use_graph_fails_before_upstream_expansion() {
    let mut html = "<svg><defs><rect id=n0 width=10 height=10 fill=red />".to_owned();
    for level in 1..18 {
        html.push_str(&format!(
            "<g id=n{level}><use href=\"#n{}\"/><use href=\"#n{}\"/></g>",
            level - 1,
            level - 1
        ));
    }
    html.push_str("</defs></svg><svg width=10 height=10><use href=\"#n17\"/></svg>");
    let page = Page::parse(&html, "https://example.test/");
    let node = page.dom.elements_named("svg").last().unwrap();
    assert!(
        serialize::source(&node, None)
            .unwrap_err()
            .contains("expansion")
    );
}

#[test]
fn percent_encoded_and_css_escaped_fragments_share_dom_lookup() {
    let image = raster(
        r##"<svg><defs><g id="shape+one"><rect width=10 height=10 fill="url('#paint%2Bone')" /></g>
        <linearGradient id="paint+one"><stop stop-color=red /><stop offset=1 stop-color=red /></linearGradient></defs></svg>
        <svg width=10 height=10><use href="#shape%2Bone" /></svg>"##,
    );
    assert_eq!(&image.bgra[20..24], [0, 0, 255, 255]);
    let image = raster(
        r##"<svg><defs><clipPath id=half><rect width=5 height=10 /></clipPath></defs></svg>
        <svg width=10 height=10><rect width=10 height=10 fill=red clip-path="url(\23 half)" /></svg>"##,
    );
    assert_eq!(&image.bgra[8..12], [0, 0, 255, 255]);
    assert_eq!(&image.bgra[28..32], [0, 0, 0, 0]);
}
