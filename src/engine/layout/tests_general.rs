use super::test_support::{CountingMeasurer, FixedMeasurer};
use super::*;

#[test]
fn lays_out_centered_image_form_and_links() {
    let mut page = Page::parse(
        r#"
            <style>body{margin:0} center{text-align:center}.logo{padding:20px 0}
            .search{width:300px;height:24px} a{color:#123456}</style>
            <center><img class="logo" src="/logo.png" width="100" height="40"><br>
            <form action="/search"><input class="search" name="q"><br>
            <input type="submit" value="Search"></form><a href="/about">About</a></center>
        "#,
        "https://example.com/",
    );
    page.images.insert(
        "https://example.com/logo.png".into(),
        super::super::page::DecodedImage {
            width: 100,
            height: 40,
            bgra: vec![0; 100 * 40 * 4],
        },
    );
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    let logo = output
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Image { rect, .. } => Some(*rect),
            _ => None,
        })
        .unwrap();
    assert!((logo.x - 350.0).abs() < 1.0);
    let controls = output
        .items
        .iter()
        .filter(|item| matches!(item, DisplayItem::Control(_)))
        .count();
    assert_eq!(controls, 2);
    assert!(output.items.iter().any(|item| matches!(item, DisplayItem::Text { link: Some(link), .. } if link == "https://example.com/about")));
}

#[test]
fn associates_external_controls_and_hidden_fields_with_their_form_owner() {
    let page = Page::parse(
        r#"<form id="search" action="/find"></form>
           <input form="search" name="q" value="rust">
           <input form="search" type="hidden" name="lang" value="en">"#,
        "https://example.com/",
    );
    let form = page.dom.elements_named("form").next().unwrap();
    let form_id = node_id(&form);
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);

    assert!(output.items.iter().any(|item| {
        matches!(item, DisplayItem::Control(control) if control.name == "q" && control.form_id == Some(form_id))
    }));
    assert_eq!(
        output.forms[&form_id].hidden_fields,
        [("lang".into(), "en".into())]
    );
}

#[test]
fn evaluates_media_queries_against_the_style_viewport() {
    let page = Page::parse(
        r#"<style>@media (min-width: 1100px) { p { color: #c00 } }</style><p>Wide</p>"#,
        "https://example.com/",
    );
    let mut measurer = FixedMeasurer;
    let output = layout_page_with_style_viewport(&page, 1080.0, 600.0, 1110.0, &mut measurer);

    assert!(output.items.iter().any(|item| {
        matches!(item, DisplayItem::Text { text, color, .. }
            if text == "Wide" && *color == Color::rgb(204, 0, 0))
    }));
}

#[test]
fn centers_explicitly_sized_background_images_in_block_boxes() {
    let mut page = Page::parse(
        r#"<style>
            body { margin: 0 }
            .logo {
                display: block;
                width: 65px;
                height: 60px;
                background: no-repeat center/auto 36px url('/logo.svg');
            }
           </style><a class="logo"></a>"#,
        "https://example.com/",
    );
    page.images.insert(
        "https://example.com/logo.svg".into(),
        super::super::page::DecodedImage {
            width: 48,
            height: 48,
            bgra: vec![0; 48 * 48 * 4],
        },
    );
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    let (clip, tile, repeat_x, repeat_y) = output
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::BackgroundImage {
                clip_rect,
                tile_rect,
                repeat_x,
                repeat_y,
                ..
            } => Some((*clip_rect, *tile_rect, *repeat_x, *repeat_y)),
            _ => None,
        })
        .unwrap();
    assert_eq!(clip.width, 65.0);
    assert_eq!(clip.height, 60.0);
    assert!((tile.x - 14.5).abs() < 0.01);
    assert!((tile.y - 12.0).abs() < 0.01);
    assert_eq!(tile.width, 36.0);
    assert_eq!(tile.height, 36.0);
    assert!(!repeat_x);
    assert!(!repeat_y);
}

#[test]
fn preserves_spaces_between_inline_elements() {
    let page = Page::parse(
        "<p>Hello <span>wide</span> world</p>",
        "https://example.com/",
    );
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    let text = output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<String>();
    assert_eq!(text, "Hello wide world");
}

#[test]
fn caches_measurements_for_nested_inline_boxes() {
    let page = Page::parse(
        r#"<p><span style="background: red"><span style="background: blue">cached measurement</span></span></p>"#,
        "https://example.com/",
    );
    let mut measurer = CountingMeasurer::default();
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    let text = output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<String>();

    assert_eq!(text, "cached measurement");
    assert_eq!(measurer.calls, 2);
}

#[test]
fn skips_intrinsic_measurement_for_a_definite_flex_basis() {
    let definite_page = Page::parse(
        r#"<div style="display:flex"><span style="width:100px">definite basis</span></div>"#,
        "https://example.com/",
    );
    let automatic_page = Page::parse(
        r#"<div style="display:flex"><span>automatic basis</span></div>"#,
        "https://example.com/",
    );
    let mut definite_measurer = CountingMeasurer::default();
    layout_page(&definite_page, 800.0, 600.0, &mut definite_measurer);
    let mut automatic_measurer = CountingMeasurer::default();
    layout_page(&automatic_page, 800.0, 600.0, &mut automatic_measurer);

    assert!(definite_measurer.calls < automatic_measurer.calls);
}

#[test]
fn vertical_margins_do_not_make_normal_inline_text_unbreakable() {
    let page = Page::parse(
        r#"<style>
            body { margin: 0 }
            .column { width: 100px }
            a { margin: 0 0 .2em }
           </style><div class="column"><a href="/result">alpha beta gamma delta</a></div>"#,
        "https://example.com/",
    );
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    let lines = output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text {
                rect,
                link: Some(link),
                ..
            } if link == "https://example.com/result" => Some(rect.y),
            _ => None,
        })
        .fold(Vec::<f32>::new(), |mut lines, y| {
            if !lines.iter().any(|line| (line - y).abs() < 0.01) {
                lines.push(y);
            }
            lines
        });
    assert!(
        lines.len() >= 2,
        "expected wrapped inline text, got {lines:?}"
    );
}

#[test]
fn does_not_break_before_punctuation_at_inline_boundaries() {
    let page = Page::parse(
        r#"<style>body { margin: 0 } p { width: 84px; margin: 0 }</style>
           <p>alpha <b>beta</b>. gamma</p>"#,
        "https://example.com/",
    );
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    let text_items = output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { rect, text, .. } => Some((text.as_str(), rect.y)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let beta_y = text_items
        .iter()
        .find_map(|(text, y)| text.contains("beta").then_some(*y))
        .unwrap();
    let punctuation_y = text_items
        .iter()
        .find_map(|(text, y)| (*text == ".").then_some(*y))
        .unwrap();
    let gamma_y = text_items
        .iter()
        .find_map(|(text, y)| text.contains("gamma").then_some(*y))
        .unwrap();
    assert_eq!(punctuation_y, beta_y);
    assert!(gamma_y > punctuation_y);
}

#[test]
fn places_explicit_grid_items_across_fractional_and_fixed_tracks() {
    let page = Page::parse(
        r#"
            <style>
                body { margin: 0 }
                #container { display: flex }
                .grid { display: grid; width: 900px;
                        grid-template-columns: 1fr 1fr 300px }
                .main { grid-area: 1 / 1 / 2 / 3; height: 40px; background: #ff0000 }
                .side { grid-area: 1 / 3 / 2 / 4; height: 60px; background: #0000ff }
            </style>
            <div id="container"><div class="grid">
                <main class="main"></main><aside class="side"></aside>
            </div></div>
        "#,
        "https://example.com/",
    );
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 900.0, 600.0, &mut measurer);
    let main = output
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::SolidRect { rect, color, .. } if *color == Color::rgb(255, 0, 0) => {
                Some(*rect)
            }
            _ => None,
        })
        .unwrap();
    let side = output
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::SolidRect { rect, color, .. } if *color == Color::rgb(0, 0, 255) => {
                Some(*rect)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(
        main,
        RectF {
            x: 0.0,
            y: 0.0,
            width: 600.0,
            height: 40.0
        }
    );
    assert_eq!(
        side,
        RectF {
            x: 600.0,
            y: 0.0,
            width: 300.0,
            height: 60.0
        }
    );
}
