use super::*;

fn rendered_text(html: &str) -> (String, String) {
    let page = Page::parse(html, "https://example.com/");
    let source = page.dom.elements_named("p").next().unwrap().text_content();
    let rendered = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer)
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<String>();
    (source, rendered)
}

#[test]
fn uppercase_changes_visual_text_without_mutating_dom_source() {
    let (source, rendered) =
        rendered_text("<style>p{text-transform:uppercase}</style><p>Straße déjà vu</p>");
    assert_eq!(source, "Straße déjà vu");
    assert_eq!(rendered, "STRASSE DÉJÀ VU");
}

#[test]
fn lowercase_expansion_and_surrogates_do_not_change_dom_source() {
    let (source, rendered) =
        rendered_text("<style>p{text-transform:lowercase}</style><p>İSTANBUL 😀</p>");
    assert_eq!(source, "İSTANBUL 😀");
    assert_eq!(rendered, "i\u{307}stanbul 😀");
}

#[test]
fn capitalization_uses_word_boundaries_across_inline_elements() {
    let (source, rendered) = rendered_text(
        "<style>p{text-transform:capitalize}</style><p>he<b>llo wo</b>rld, déjà vu</p>",
    );
    assert_eq!(source, "hello world, déjà vu");
    assert_eq!(rendered, "Hello World, Déjà Vu");
}

#[test]
fn descendants_can_override_inherited_case_conversion() {
    let (source, rendered) = rendered_text(
        "<style>p{text-transform:uppercase}b{text-transform:none}i{text-transform:lowercase}</style>\
         <p>UP <b>kept</b> <i>LOWER</i></p>",
    );
    assert_eq!(source, "UP kept LOWER");
    assert_eq!(rendered, "UP kept lower");
}

#[test]
fn selection_ranges_stay_addressed_by_source_utf16_offsets() {
    let page = Page::parse(
        "<style>body{margin:0}p{margin:0;text-transform:uppercase}</style><p>ßx😀z</p>",
        "https://example.com/",
    );
    let paragraph = page.dom.elements_named("p").next().unwrap();
    let text = paragraph.children.borrow()[0].clone();
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    assert_eq!(text.text_content(), "ßx😀z");
    let painted = output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<String>();
    assert_eq!(painted, "SSX😀Z");

    // ß expands to two visual glyphs, yet both retain the original [0, 1)
    // source range; the non-BMP emoji remains one [2, 4) source character.
    let sharp_s = output.fragments.range_text(text.id(), 0, 1);
    let x = output.fragments.range_text(text.id(), 1, 2);
    let emoji = output.fragments.range_text(text.id(), 2, 4);
    let z = output.fragments.range_text(text.id(), 4, 5);
    assert_eq!(sharp_s.len(), 1);
    assert_eq!(x.len(), 1);
    assert_eq!(emoji.len(), 1);
    assert_eq!(z.len(), 1);
    assert_eq!(sharp_s[0].x, 0.0);
    assert_eq!(sharp_s[0].width, 16.0);
    assert_eq!(x[0].x, sharp_s[0].right());
    assert_eq!(emoji[0].x, x[0].right());
    assert_eq!(z[0].x, emoji[0].right());
}

#[test]
fn transformed_glyphs_drive_inline_measurement_and_following_positions() {
    let page = Page::parse(
        "<style>body{margin:0}p{margin:0;text-transform:uppercase}</style><p>ß<span>x</span></p>",
        "https://example.com/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let text_items = output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, rect, .. } => Some((text.as_str(), *rect)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(text_items.len(), 2);
    assert_eq!(text_items[0].0, "SS");
    assert_eq!(text_items[0].1.width, 16.0);
    assert_eq!(text_items[1].0, "X");
    assert_eq!(text_items[1].1.x, text_items[0].1.right());
}

#[test]
fn generated_content_receives_its_own_cascaded_text_transform() {
    let page = Page::parse(
        "<style>p{text-transform:uppercase}\
         p::before{content:'hello ';text-transform:capitalize}\
         p::after{content:' THERE';text-transform:lowercase}</style><p>world</p>",
        "https://example.com/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let painted = output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<String>();
    assert_eq!(painted, "Hello WORLD there");
}

#[test]
fn mixed_case_styles_do_not_let_a_previous_run_change_dom_text() {
    let page = Page::parse(
        "<style>p{text-transform:uppercase}b{text-transform:none}</style>\
         <p>part <b>mixed Case</b> end</p>",
        "https://example.com/",
    );
    let paragraph = page.dom.elements_named("p").next().unwrap();
    let before = paragraph.text_content();
    for _ in 0..2 {
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let painted = output
            .items
            .iter()
            .filter_map(|item| match item {
                DisplayItem::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert_eq!(painted, "PART mixed Case END");
        assert_eq!(paragraph.text_content(), before);
    }
}
