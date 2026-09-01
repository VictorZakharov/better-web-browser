use super::test_support::FixedMeasurer;
use super::*;

#[test]
fn uses_the_resolved_flex_main_size_without_applying_percentage_width_twice() {
    let page = Page::parse(
        r#"<style>
            body { margin: 0 }
            .row { display: flex; width: 400px }
            .image { display: flex; width: 62.5%; padding-right: 8px; flex: 0 0 auto }
            .thumbnail { display: block; position: relative; width: 100%; height: 0;
                         padding-top: 56.25%; overflow: hidden }
            .metadata { width: 300px; flex-shrink: 1 }
        </style>
        <div class=row>
          <a id=image class=image><span id=thumbnail class=thumbnail></span></a>
          <div class=metadata>metadata that may shrink</div>
        </div>"#,
        "https://example.com/",
    );
    let image = page
        .dom
        .elements_named("a")
        .find(|node| node.attr("id").as_deref() == Some("image"))
        .unwrap();
    let thumbnail = page
        .dom
        .elements_named("span")
        .find(|node| node.attr("id").as_deref() == Some("thumbnail"))
        .unwrap();

    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let image_rect = output.node_bounds[&image.id()];
    let thumbnail_rect = output.node_bounds[&thumbnail.id()];

    // The flex base size is 62.5% of 400px plus 8px padding. Passing 258px back as
    // a new percentage basis would incorrectly resolve the item to 169.25px.
    assert!((image_rect.width - 258.0).abs() < 0.01, "{image_rect:?}");
    assert!(
        (thumbnail_rect.width - 250.0).abs() < 0.01,
        "{thumbnail_rect:?}"
    );
    assert!(
        (thumbnail_rect.height - 140.625).abs() < 0.01,
        "{thumbnail_rect:?}"
    );
}

#[test]
fn resolves_inline_replaced_percentages_against_a_definite_containing_block() {
    let mut page = Page::parse(
        r#"<style>
            body { margin: 0 }
            .host { display: block; position: relative; width: 250px; height: 140px }
            img { display: inline-block; width: 100%; height: 100% }
        </style>
        <div class=host><img src="/thumbnail.jpg"></div>"#,
        "https://example.com/",
    );
    page.images.insert(
        "https://example.com/thumbnail.jpg".into(),
        crate::engine::page::DecodedImage {
            width: 336,
            height: 188,
            bgra: vec![0; 336 * 188 * 4],
        },
    );
    let image = page.dom.elements_named("img").next().unwrap();

    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let rect = output.node_bounds[&image.id()];

    assert_eq!(rect.width, 250.0);
    assert_eq!(rect.height, 140.0);
    assert!(output.items.iter().any(
        |item| matches!(item, DisplayItem::Image { rect, .. } if *rect == output.node_bounds[&image.id()])
    ));
}
