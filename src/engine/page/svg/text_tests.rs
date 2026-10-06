use super::*;
use crate::engine::FontSpec;
use crate::engine::font::{WebFont, shaping::FontCatalog};
use unicode_script::Script;

fn ink_bounds(image: &DecodedImage) -> (u32, u32, u32, u32) {
    let mut left = image.width;
    let mut top = image.height;
    let mut right = 0;
    let mut bottom = 0;
    for (index, pixel) in image.bgra.chunks_exact(4).enumerate() {
        if pixel[3] == 0 {
            continue;
        }
        let x = index as u32 % image.width;
        let y = index as u32 / image.width;
        left = left.min(x);
        top = top.min(y);
        right = right.max(x);
        bottom = bottom.max(y);
    }
    (left, top, right, bottom)
}

fn spec(family: &str) -> FontSpec {
    FontSpec {
        family: family.into(),
        size: 40.0,
        weight: 400,
        italic: false,
        underline: false,
        letter_spacing: 0.0,
        word_spacing: 0.0,
        rtl: false,
        kerning: true,
        features: Default::default(),
        variants: Default::default(),
    }
}

#[test]
fn svg_text_draws_real_system_font_outlines_and_respects_paint() {
    let image = decode_svg(
        br#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="80">
        <text x="10" y="55" font-family="Arial" font-size="40" fill="red">SVG</text></svg>"#,
        "text fixture",
    )
    .unwrap();
    let (left, top, right, bottom) = ink_bounds(&image);
    assert!(left >= 10 && right > 75 && right < 120);
    assert!(top > 10 && top < 35 && bottom > 45 && bottom < 60);
    let red = image
        .bgra
        .chunks_exact(4)
        .filter(|pixel| pixel[2] > 0 && pixel[0] == 0 && pixel[1] == 0)
        .count();
    assert!(red > 300, "font outlines were not painted: {red}");
}

#[test]
fn text_length_anchor_transform_and_tspan_share_upstream_layout() {
    let image = decode_svg(br#"<svg xmlns="http://www.w3.org/2000/svg" width="240" height="100">
        <g transform="translate(10 10)"><text x="180" y="55" text-anchor="end" font-family="Arial" font-size="40"
        textLength="120" lengthAdjust="spacingAndGlyphs"><tspan fill="blue">SVG</tspan></text></g></svg>"#, "positioned text").unwrap();
    let (left, top, right, bottom) = ink_bounds(&image);
    assert!((68..=75).contains(&left), "left={left}");
    assert!((185..=190).contains(&right), "right={right}");
    assert!(top > 20 && bottom > 55 && bottom < 70);
    assert!(
        image
            .bgra
            .chunks_exact(4)
            .any(|pixel| pixel[0] > 200 && pixel[2] == 0)
    );
}

#[test]
fn shared_text_definition_renders_through_a_sibling_use() {
    let page = Page::parse(
        r##"<svg width=0 height=0><defs><text id=label y=55 font-size=40 font-family=Arial>SVG</text></defs></svg>
        <svg width=200 height=80><use href="#label" x=10 fill=red /></svg>"##,
        "https://example.test/",
    );
    let node = page.dom.elements_named("svg").last().unwrap();
    let image = decode_inline_svg(&node, None).unwrap();
    assert!(
        image
            .bgra
            .chunks_exact(4)
            .filter(|pixel| pixel[3] > 0)
            .count()
            > 300
    );
    assert!(
        image
            .bgra
            .chunks_exact(4)
            .any(|pixel| pixel[2] > 200 && pixel[0] == 0)
    );
}

#[test]
fn document_web_font_alias_is_resolved_without_using_its_internal_family_name() {
    let mut catalog = FontCatalog::new();
    let selected = catalog
        .select(
            "Times New Roman",
            &spec("Times New Roman"),
            Script::Latin,
            "SVG",
        )
        .unwrap();
    let font = WebFont {
        family: "BreezeSvgAlias".into(),
        weight: 400,
        italic: false,
        sfnt: selected.font.blob.as_ref().to_vec().into(),
        source_url: "test-font:svg-alias".into(),
        script_source_id: None,
        unicode_ranges: Default::default(),
        features: Default::default(),
    };
    let source = br#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="80"><text x="10" y="55" font-family="BreezeSvgAlias" font-size="40">SVG</text></svg>"#;
    let with_alias = decode_svg_with_fonts(
        source,
        "alias",
        crate::engine::image_decode::DecodeLimits::PAGE,
        &[font],
    )
    .unwrap();
    let expected = decode_svg(br#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="80"><text x="10" y="55" font-family="Times New Roman" font-size="40">SVG</text></svg>"#, "system serif").unwrap();
    assert_eq!(with_alias.bgra, expected.bgra);
    assert_ne!(
        with_alias.bgra,
        decode_svg(source, "missing alias").unwrap().bgra
    );
}

#[test]
fn loaded_web_fonts_invalidate_text_rasters_but_not_shape_only_rasters() {
    let page = Page::parse(
        "<svg width=200 height=80><text y=55 font-size=40>SVG</text></svg>",
        "https://example.test/",
    );
    let node = page.dom.elements_named("svg").next().unwrap();
    let original = InlineSvgInput::new(&node, None).version;
    let font = WebFont {
        family: "Fixture".into(),
        weight: 400,
        italic: false,
        sfnt: vec![0; 12].into(),
        source_url: "test-font:stamp".into(),
        script_source_id: None,
        unicode_ranges: Default::default(),
        features: Default::default(),
    };
    assert_ne!(
        InlineSvgInput::with_fonts(&node, None, std::slice::from_ref(&font)).version,
        original
    );
    let shape = Page::parse(
        "<svg width=10 height=10><rect width=10 height=10 /></svg>",
        "https://example.test/",
    );
    let node = shape.dom.elements_named("svg").next().unwrap();
    assert_eq!(
        InlineSvgInput::new(&node, None).version,
        InlineSvgInput::with_fonts(&node, None, &[font]).version
    );
}

#[test]
fn unicode_text_uses_real_fallback_and_bidi_outlines() {
    let source = r#"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="90">
        <text x="10" y="55" font-family="Arial" font-size="36">Hello שלום سلام नमस्ते</text></svg>"#;
    let image = decode_svg(source.as_bytes(), "Unicode text").unwrap();
    let ink = image
        .bgra
        .chunks_exact(4)
        .filter(|pixel| pixel[3] > 0)
        .count();
    assert!(ink > 1000 && ink_bounds(&image).2 > 250, "ink={ink}");
    assert_eq!(
        image.bgra,
        decode_svg(source.as_bytes(), "repeat Unicode")
            .unwrap()
            .bgra
    );
}

#[test]
fn html_ancestor_and_document_stylesheet_fonts_reach_svg_rasterization() {
    let mut page = Page::parse(
        r#"<style>section{font-family:Arial;font-size:20px} .label{font-size:40px}</style>
        <section><svg width=200 height=80><text class=label x=10 y=55>SVG</text></svg></section>"#,
        "https://example.test/",
    );
    page.refresh_resources(800.0);
    let svg = page.dom.elements_named("svg").next().unwrap();
    let image = &page.images[&inline_svg_key(&svg)];
    let expected = decode_svg(br#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="80"><text x="10" y="55" font-family="Arial" font-size="40">SVG</text></svg>"#, "explicit font").unwrap();
    assert_eq!(image.bgra, expected.bgra);
}
