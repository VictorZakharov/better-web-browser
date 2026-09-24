use super::*;
use crate::engine::page::Page;

fn pixel(image: &DecodedImage, x: usize) -> &[u8] {
    &image.bgra[x * 4..x * 4 + 4]
}

#[test]
fn svg_current_color_preserves_explicit_fills_and_tracks_ancestor_styles() {
    let mut page = Page::parse(
        r#"<style>section{color:blue}.changed{color:green}#local{color:yellow}</style>
        <section><svg width=30 height=10><rect width=10 height=10 fill=red />
        <rect x=10 width=10 height=10 fill=currentColor style="" />
        <rect id=local x=20 width=10 height=10 fill=currentColor /></svg></section>"#,
        "https://example.test/",
    );
    page.refresh_resources(800.0);
    let svg = page.dom.elements_named("svg").next().unwrap();
    let key = inline_svg_key(&svg);
    assert_eq!(
        page.cached_style(800.0).unwrap().get(&svg).color,
        crate::engine::css::Color::rgb(0, 0, 255)
    );
    assert_eq!(pixel(&page.images[&key], 5), [0, 0, 255, 255]);
    assert_eq!(pixel(&page.images[&key], 15), [255, 0, 0, 255]);
    assert_eq!(pixel(&page.images[&key], 25), [0, 255, 255, 255]);
    let before = page.images[&key].bgra.clone();
    page.refresh_resources(800.0);
    assert!(
        std::sync::Arc::ptr_eq(&before, &page.images[&key].bgra),
        "unchanged raster must be reused"
    );
    page.dom
        .elements_named("section")
        .next()
        .unwrap()
        .set_attr("class", "changed");
    page.refresh_resources(800.0);
    assert_eq!(pixel(&page.images[&key], 5), [0, 0, 255, 255]);
    assert_eq!(pixel(&page.images[&key], 15), [0, 128, 0, 255]);
    assert_eq!(pixel(&page.images[&key], 25), [0, 255, 255, 255]);
}

#[test]
fn svg_inline_color_and_presentation_color_are_resolved_per_element() {
    let mut page = Page::parse(
        r#"<svg width=30 height=10 color=red>
        <rect width=10 height=10 fill=currentColor />
        <g color=green><rect x=10 width=10 height=10 fill=currentColor /></g>
        <rect style="color:blue;" x=20 width=10 height=10 fill=currentColor /></svg>"#,
        "https://example.test/",
    );
    page.refresh_resources(800.0);
    let svg = page.dom.elements_named("svg").next().unwrap();
    let image = &page.images[&inline_svg_key(&svg)];
    assert_eq!(pixel(image, 5), [0, 0, 255, 255]);
    assert_eq!(pixel(image, 15), [0, 128, 0, 255]);
    assert_eq!(pixel(image, 25), [255, 0, 0, 255]);
}

#[test]
fn svg_color_matrix_filter_changes_the_rendered_pixel() {
    let mut page = Page::parse(
        r#"<svg width="10" height="10" xmlns="http://www.w3.org/2000/svg">
        <defs><filter id="gray"><feColorMatrix type="saturate" values="0"/></filter></defs>
        <rect width="10" height="10" fill="red" filter="url(#gray)"/></svg>"#,
        "https://example.test/",
    );
    page.refresh_resources(800.0);
    let svg = page.dom.elements_named("svg").next().unwrap();
    let image = &page.images[&inline_svg_key(&svg)];
    let filtered = pixel(image, 5);
    assert_eq!(filtered[0], filtered[1]);
    assert_eq!(filtered[1], filtered[2]);
    assert_eq!(filtered[3], 255);
}

#[test]
fn svg_offset_filter_moves_pixels_and_respects_result_chains() {
    let mut page = Page::parse(
        r#"<svg width="20" height="20" xmlns="http://www.w3.org/2000/svg">
        <defs><filter id="shift" x="-100%" width="300%">
        <feOffset dx="5" dy="0" result="moved"/>
        <feComposite in="moved" in2="moved" operator="over"/>
        </filter></defs>
        <rect x="2" y="2" width="5" height="5" fill="red" filter="url(#shift)"/>
        </svg>"#,
        "https://example.test/",
    );
    page.refresh_resources(800.0);
    let svg = page.dom.elements_named("svg").next().unwrap();
    let image = &page.images[&inline_svg_key(&svg)];
    assert_eq!(pixel(image, 4 * 20 + 3)[3], 0);
    assert_eq!(pixel(image, 4 * 20 + 9), [0, 0, 255, 255]);
}
