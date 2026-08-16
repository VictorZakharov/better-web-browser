use better_web_browser::engine::dom::{self, Node};
use better_web_browser::engine::page::Page;
use better_web_browser::fuzzing;
use better_web_browser::limits::{
    MAX_CSS_SOURCE_BYTES, MAX_DECODED_IMAGE_DIMENSION, MAX_DOM_DEPTH, MAX_FONT_BYTES,
    MAX_SCRIPT_BYTES, MAX_SVG_SOURCE_BYTES, MAX_URL_BYTES,
};
use better_web_browser::navigation::{normalize_user_input, resolve_url};

type FuzzCase = (fn(&[u8]), &'static [u8]);

#[test]
fn committed_fuzz_corpus_replays_without_panics() {
    let cases: &[FuzzCase] = &[
        (
            fuzzing::html_document,
            include_bytes!("../fuzz/corpus/html_document/formatting-and-table.html"),
        ),
        (
            fuzzing::html_document,
            include_bytes!("../fuzz/corpus/html_document/foreign-content.html"),
        ),
        (
            fuzzing::html_fragment,
            include_bytes!("../fuzz/corpus/html_fragment/misnested.html"),
        ),
        (
            fuzzing::html_fragment,
            include_bytes!("../fuzz/corpus/html_fragment/template.html"),
        ),
        (
            fuzzing::css_stylesheet,
            include_bytes!("../fuzz/corpus/css_stylesheet/nested.css"),
        ),
        (
            fuzzing::css_stylesheet,
            include_bytes!("../fuzz/corpus/css_stylesheet/recovery.css"),
        ),
        (
            fuzzing::url_resolution,
            include_bytes!("../fuzz/corpus/url_resolution/unicode.txt"),
        ),
        (
            fuzzing::url_resolution,
            include_bytes!("../fuzz/corpus/url_resolution/invalid.txt"),
        ),
        (
            fuzzing::dom_mutations,
            include_bytes!("../fuzz/corpus/dom_mutations/mixed.ops"),
        ),
        (
            fuzzing::javascript_host_bindings,
            include_bytes!("../fuzz/corpus/javascript_host_bindings/mixed.ops"),
        ),
    ];

    for (target, input) in cases {
        target(input);
    }
}

#[test]
fn deeply_nested_html_is_pruned_to_the_document_budget() {
    let mut html = "<main>".to_string();
    for _ in 0..MAX_DOM_DEPTH + 64 {
        html.push_str("<div>");
    }
    html.push_str("payload");

    let parsed = dom::parse(&html);
    let deepest = Node::descendants(&parsed.document)
        .map(|node| {
            let mut depth = 0;
            let mut ancestor = node.parent();
            while let Some(parent) = ancestor {
                depth += 1;
                ancestor = parent.parent();
            }
            depth
        })
        .max()
        .unwrap();

    assert!(deepest <= MAX_DOM_DEPTH);
    assert!(
        parsed
            .errors
            .borrow()
            .iter()
            .any(|error| error.starts_with("safety limit:"))
    );
}

#[test]
fn oversized_urls_and_page_resources_fail_softly() {
    let oversized_url = "x".repeat(MAX_URL_BYTES + 1);
    assert!(normalize_user_input(&oversized_url).is_err());
    assert!(resolve_url("https://example.test/", &oversized_url).is_none());

    let mut page = Page::parse("<script src='/large.js'></script>", "https://example.test/");
    assert!(!page.add_script(
        "https://example.test/large.js",
        "x".repeat(MAX_SCRIPT_BYTES + 1),
    ));
    assert!(page.add_stylesheet("a".repeat(MAX_CSS_SOURCE_BYTES + 1)));
    assert_eq!(page.external_stylesheets[0].len(), MAX_CSS_SOURCE_BYTES);
    assert_eq!(page.diagnostics.len(), 2);
}

#[test]
fn extreme_css_nesting_and_host_operations_remain_bounded() {
    let css = format!(
        "{}a {{ color: red; }}{}",
        "@media (min-width: 1px) {{".repeat(96),
        "}".repeat(96)
    );
    fuzzing::css_stylesheet(css.as_bytes());
    fuzzing::dom_mutations(&[0; 4_096]);
    fuzzing::javascript_host_bindings(&[0; 4_096]);
}

#[test]
fn media_decoders_reject_oversized_headers_before_output_allocation() {
    let mut page = Page::parse("", "https://example.test/");
    let dimension = MAX_DECODED_IMAGE_DIMENSION + 1;
    let mut bmp = vec![0_u8; 54];
    bmp[0..2].copy_from_slice(b"BM");
    bmp[2..6].copy_from_slice(&(54_u32).to_le_bytes());
    bmp[10..14].copy_from_slice(&(54_u32).to_le_bytes());
    bmp[14..18].copy_from_slice(&(40_u32).to_le_bytes());
    bmp[18..22].copy_from_slice(&dimension.to_le_bytes());
    bmp[22..26].copy_from_slice(&dimension.to_le_bytes());
    bmp[26..28].copy_from_slice(&(1_u16).to_le_bytes());
    bmp[28..30].copy_from_slice(&(24_u16).to_le_bytes());
    assert!(page.add_image("oversized.bmp".into(), &bmp).is_err());

    let mut svg = vec![b' '; MAX_SVG_SOURCE_BYTES + 1];
    svg[..5].copy_from_slice(b"<svg>");
    assert!(page.add_image("oversized.svg".into(), &svg).is_err());

    let mut font = vec![0_u8; MAX_FONT_BYTES + 1];
    font[..4].copy_from_slice(b"OTTO");
    assert!(
        page.add_font("oversized.otf".into(), "Hostile".into(), 400, false, &font,)
            .is_err()
    );
}
