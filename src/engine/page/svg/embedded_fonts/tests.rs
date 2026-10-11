use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};

fn source(css: &str) -> String {
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='100' height='40'><style>{css}</style><text x='0' y='25' font-family='EmbeddedAlias' font-size='20'>XX</text></svg>"
    )
}

fn face() -> String {
    // Existing, vetted repository font fixture; no imported font or new decoder.
    format!(
        "@font-face{{font-family:EmbeddedAlias;src:url(data:font/ttf;base64,{})}}",
        STANDARD.encode(include_bytes!("../../../../../tests/canvas/fonts/ahem.ttf"))
    )
}

#[test]
fn embedded_svg_font_uses_shared_descriptors_and_validated_sfnt_bytes() {
    let source = source(&face());
    let document = super::super::admission::document(source.as_bytes()).unwrap();
    let fonts = collect(&document);
    assert_eq!(fonts.len(), 1);
    assert_eq!(fonts[0].family, "EmbeddedAlias");
    assert_eq!(
        fonts[0].sfnt.as_ref(),
        include_bytes!("../../../../../tests/canvas/fonts/ahem.ttf")
    );
}

#[test]
fn svg_image_font_loader_never_reads_remote_or_local_files_or_invalid_fonts() {
    for candidate in [
        "https://example.invalid/font.woff2",
        "file:///C:/Windows/Fonts/arial.ttf",
        "../font.ttf",
        "data:font/woff2;base64,AA==",
    ] {
        let source = source(&format!(
            "@font-face{{font-family:EmbeddedAlias;src:url({candidate})}}"
        ));
        let document = super::super::admission::document(source.as_bytes()).unwrap();
        assert!(collect(&document).is_empty(), "{candidate}");
    }
    let source = source(&format!("/* {} */ .x{{content:'@font-face'}}", face()));
    let document = super::super::admission::document(source.as_bytes()).unwrap();
    assert!(collect(&document).is_empty());
}

#[test]
fn embedded_font_fallback_and_face_budget_are_bounded() {
    let css = face().replace(
        "src:url(",
        "src:url(https://example.invalid/no-font.woff2),url(",
    );
    let source = source(&css.repeat(MAX_FACES + 1));
    let document = super::super::admission::document(source.as_bytes()).unwrap();
    assert_eq!(collect(&document).len(), MAX_FACES);
}

#[cfg(windows)]
#[test]
fn embedded_font_changes_actual_svg_pixels_and_matches_explicit_font_input() {
    let with_font = source(&face());
    let without_font = source("");
    let document = super::super::admission::document(with_font.as_bytes()).unwrap();
    let fonts = collect(&document);
    let limits = crate::engine::image_decode::DecodeLimits::CANVAS;
    let embedded =
        super::super::decode_svg_with_limits(with_font.as_bytes(), "embedded", limits).unwrap();
    let explicit =
        super::super::decode_svg_with_fonts(without_font.as_bytes(), "explicit", limits, &fonts)
            .unwrap();
    let fallback =
        super::super::decode_svg_with_limits(without_font.as_bytes(), "fallback", limits).unwrap();
    assert_eq!(embedded.bgra, explicit.bgra);
    assert_ne!(embedded.bgra, fallback.bgra);
    assert!(
        embedded
            .bgra
            .chunks_exact(4)
            .filter(|pixel| pixel[3] > 0)
            .count()
            > 500
    );
}
