use super::*;
use crate::engine::page::Page;

#[test]
fn root_compositing_styles_do_not_modify_or_invalidate_local_svg_pixels() {
    let mut page = Page::parse(
        "<svg width=20 height=10 style='opacity:.5;transform:scaleX(.5);clip-path:inset(0 50% 0 0)'><rect width=20 height=10 fill=red /></svg>",
        "https://example.test/",
    );
    page.refresh_resources(800.0);
    let svg = page.dom.elements_named("svg").next().unwrap();
    let key = inline_svg_key(&svg);
    let pixels = page.images[&key].bgra.clone();
    assert_eq!(&pixels[19 * 4..20 * 4], &[0, 0, 255, 255]);
    svg.set_attr(
        "style",
        "opacity:.25;transform:rotate(20deg);clip-path:inset(0 75% 0 0)",
    );
    page.refresh_resources(800.0);
    assert!(std::sync::Arc::ptr_eq(&pixels, &page.images[&key].bgra));
    svg.set_attr("style", "fill:blue;opacity:.25");
    page.dom
        .elements_named("rect")
        .next()
        .unwrap()
        .remove_attr("fill");
    page.refresh_resources(800.0);
    assert!(!std::sync::Arc::ptr_eq(&pixels, &page.images[&key].bgra));
    assert_eq!(&page.images[&key].bgra[19 * 4..20 * 4], &[255, 0, 0, 255]);
}

#[test]
fn descendant_svg_opacity_and_attribute_transform_still_belong_to_the_raster() {
    let mut page = Page::parse(
        "<svg width=20 height=10><g transform='translate(10 0)' style='opacity:.5'><rect width=10 height=10 fill=red /></g></svg>",
        "https://example.test/",
    );
    page.refresh_resources(800.0);
    let svg = page.dom.elements_named("svg").next().unwrap();
    let pixels = &page.images[&inline_svg_key(&svg)].bgra;
    assert_eq!(pixels[5 * 4 + 3], 0);
    assert!((127..=128).contains(&pixels[15 * 4 + 3]));
}

#[test]
fn root_stylesheet_opacity_is_composed_by_css_not_baked_into_the_local_raster() {
    let mut page = Page::parse(
        "<svg width=20 height=10><style>svg{opacity:.5}</style><rect width=20 height=10 fill=red /></svg>",
        "https://example.test/",
    );
    page.refresh_resources(800.0);
    let svg = page.dom.elements_named("svg").next().unwrap();
    assert_eq!(
        page.cached_styles.as_ref().unwrap().2.get(&svg).opacity,
        0.5
    );
    let pixels = &page.images[&inline_svg_key(&svg)].bgra;
    assert_eq!(pixels[5 * 4 + 3], 255, "CSS owns the outer opacity once");
}

#[test]
fn root_stylesheet_transform_does_not_double_transform_the_local_raster() {
    let mut page = Page::parse(
        "<svg width=20 height=10><style>svg{transform:scale(.5);transform-origin:0 0}</style><rect width=20 height=10 fill=red /></svg>",
        "https://example.test/",
    );
    page.refresh_resources(800.0);
    let svg = page.dom.elements_named("svg").next().unwrap();
    let pixels = &page.images[&inline_svg_key(&svg)].bgra;
    assert_eq!(pixels[15 * 4 + 3], 255, "CSS owns the root scale once");
}

#[test]
fn root_effect_attributes_remain_visible_to_descendant_selectors() {
    let mut page = Page::parse(
        "<svg width=20 height=10 opacity='.5' transform='scale(.5)'><style>svg[opacity][transform] rect{fill:red}</style><rect width=20 height=10 /></svg>",
        "https://example.test/",
    );
    page.refresh_resources(800.0);
    let svg = page.dom.elements_named("svg").next().unwrap();
    let style = page.cached_styles.as_ref().unwrap().2.get(&svg);
    assert!(style.has_transform());
    assert_eq!(style.opacity, 0.5);
    let pixels = &page.images[&inline_svg_key(&svg)].bgra;
    assert_eq!(&pixels[15 * 4..16 * 4], &[0, 0, 255, 255]);
}

#[test]
fn root_presentation_opacity_is_resolved_by_css_while_descendants_keep_raster_opacity() {
    for (markup, expected_opacity) in [
        ("opacity='.5'", 0.5),
        ("opacity='.5' style='opacity:1'", 1.0),
        ("opacity='.5' style='opacity:.8'", 0.8),
    ] {
        let mut page = Page::parse(
            &format!(
                "<svg width=20 height=10 {markup}><rect width=20 height=10 fill=red opacity='.5' /></svg>"
            ),
            "https://example.test/",
        );
        page.refresh_resources(800.0);
        let svg = page.dom.elements_named("svg").next().unwrap();
        assert_eq!(
            page.cached_styles.as_ref().unwrap().2.get(&svg).opacity,
            expected_opacity
        );
        let pixels = &page.images[&inline_svg_key(&svg)].bgra;
        assert!((127..=128).contains(&pixels[5 * 4 + 3]), "{markup}");
    }
}

#[test]
fn composition_filter_preserves_svg_paints_strings_custom_values_and_priorities() {
    let result = crate::engine::css::cssom::declarations::without_properties(
        r#"fill:url('#a;b')!important;--label:'a;b';OPACITY:.5;t\72 ansform:scale(2);stroke:rgb(1,2,3)"#,
        &["opacity", "transform"],
    );
    assert_eq!(
        result,
        "fill:url('#a;b')!important;--label:'a;b';stroke:rgb(1,2,3);"
    );
}

#[test]
fn url_clips_and_masks_remain_owned_by_the_svg_decoder() {
    let mut page = Page::parse(
        "<svg width=20 height=10 style='clip-path:url(#cut);mask:url(#fade)'><defs><clipPath id=cut><rect width=10 height=10 /></clipPath><mask id=fade><rect width=20 height=10 fill=white /></mask></defs><rect width=20 height=10 fill=red /></svg>",
        "https://example.test/",
    );
    page.refresh_resources(800.0);
    let svg = page.dom.elements_named("svg").next().unwrap();
    let pixels = &page.images[&inline_svg_key(&svg)].bgra;
    assert_eq!(pixels[5 * 4 + 3], 255);
    assert_eq!(pixels[15 * 4 + 3], 0);
}
