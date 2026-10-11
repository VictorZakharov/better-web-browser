use super::*;

fn pixels(html: &str) -> std::sync::Arc<[u8]> {
    let mut page = Page::parse(html, "https://example.test/");
    page.refresh_resources(800.0);
    let svg = page.dom.elements_named("svg").next().unwrap();
    page.images[&inline_svg_key(&svg)].bgra.clone()
}

#[test]
fn outer_document_rules_match_svg_attributes_and_ancestors() {
    let pixels = pixels(
        "<style>.host svg[opacity][transform] rect{fill:red}</style><section class=host><svg width=20 height=10 opacity='.5' transform='scale(.5)'><rect width=20 height=10 /></svg></section>",
    );
    assert_eq!(&pixels[15 * 4..16 * 4], &[0, 0, 255, 255]);
}

#[test]
fn paint_uses_the_normal_cascade_variables_and_css_wide_keywords() {
    let pixels = pixels(
        "<style>@layer base{svg{--ink:blue;fill:var(--ink)}rect{fill:red}}@layer overrides{rect{fill:revert-layer}}#initial{fill:initial}#none{fill:none!important}</style><svg width=30 height=10><rect width=10 height=10 fill=green /><rect id=initial x=10 width=10 height=10 /><rect id=none x=20 width=10 height=10 style='fill:blue' /></svg>",
    );
    assert_eq!(&pixels[5 * 4..6 * 4], &[0, 0, 255, 255]);
    assert_eq!(&pixels[15 * 4..16 * 4], &[0, 0, 0, 255]);
    assert_eq!(pixels[25 * 4 + 3], 0);
}

#[test]
fn current_color_inherits_as_a_reference_to_each_elements_color() {
    let pixels = pixels(
        "<style>svg{fill:currentColor;color:red}#blue{color:blue}</style><svg width=20 height=10><rect width=10 height=10 /><rect id=blue x=10 width=10 height=10 /></svg>",
    );
    assert_eq!(&pixels[5 * 4..6 * 4], &[0, 0, 255, 255]);
    assert_eq!(&pixels[15 * 4..16 * 4], &[255, 0, 0, 255]);
}

#[test]
fn paint_servers_and_fallbacks_survive_css_serialization() {
    let pixels = pixels(
        "<style>rect{fill:url('#red')}#fallback{fill:url(#missing) blue}</style><svg width=20 height=10><defs><linearGradient id=red><stop stop-color=red /><stop offset=1 stop-color=red /></linearGradient></defs><rect width=10 height=10 /><rect id=fallback x=10 width=10 height=10 /></svg>",
    );
    assert_eq!(&pixels[5 * 4..6 * 4], &[0, 0, 255, 255]);
    assert_eq!(&pixels[15 * 4..16 * 4], &[255, 0, 0, 255]);
}

#[test]
fn definitions_keep_instance_paint_inheritance() {
    let pixels = pixels(
        "<style>use{fill:red}#second{fill:blue}</style><svg width=20 height=10><defs><rect id=shape width=10 height=10 /></defs><use href='#shape' /><use id=second x=10 href='#shape' /></svg>",
    );
    assert_eq!(&pixels[5 * 4..6 * 4], &[0, 0, 255, 255]);
    assert_eq!(&pixels[15 * 4..16 * 4], &[255, 0, 0, 255]);
}

#[test]
fn selector_paint_changes_invalidate_but_root_compositor_changes_reuse_pixels() {
    let mut page = Page::parse(
        "<style>.red rect{fill:red}.blue rect{fill:blue}</style><svg class=red width=10 height=10><rect width=10 height=10 /></svg>",
        "https://example.test/",
    );
    page.refresh_resources(800.0);
    let svg = page.dom.elements_named("svg").next().unwrap();
    let key = inline_svg_key(&svg);
    let before = page.images[&key].bgra.clone();
    svg.set_attr("style", "opacity:.5;transform:scale(.5)");
    page.refresh_resources(800.0);
    assert!(std::sync::Arc::ptr_eq(&before, &page.images[&key].bgra));
    svg.set_attr("class", "blue");
    page.refresh_resources(800.0);
    let after = &page.images[&key].bgra;
    assert!(!std::sync::Arc::ptr_eq(&before, after));
    assert_eq!(&after[5 * 4..6 * 4], &[255, 0, 0, 255]);
}

#[test]
fn document_stroke_paint_overrides_presentation_attributes() {
    let pixels = pixels(
        "<style>rect{stroke:red}</style><svg width=10 height=10><rect x=1 y=1 width=8 height=8 fill=none stroke=blue stroke-width=2 /></svg>",
    );
    assert_eq!(&pixels[5 * 4..6 * 4], &[0, 0, 255, 255]);
    assert_eq!(pixels[(5 * 10 + 5) * 4 + 3], 0);
}
