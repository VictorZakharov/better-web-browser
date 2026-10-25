use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

#[test]
fn short_body_background_does_not_replace_an_opaque_html_canvas() {
    // CSS Backgrounds 3 §2.11.2: body propagation requires a transparent HTML
    // background. This is also why asynchronous loading fixtures size the body.
    let page = Page::parse(
        "<style>html{background:red}body{margin:0;height:30px;background:green}</style>",
        "https://example.test/",
    );
    let body = page.dom.elements_named("body").next().unwrap();
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    assert_eq!(output.background, Color::rgb(255, 0, 0));
    let bounds = output.node_bounds[&body.id()];
    assert_eq!(bounds.height, 30.0);
    assert!(output.items.iter().any(|item| matches!(item,
        DisplayItem::SolidRect { rect, color, .. }
            if *rect == bounds && *color == Color::rgb(0, 128, 0)
    )));
}

#[test]
fn background_position_uses_padding_box_but_paints_under_the_border() {
    let page = Page::parse(
        "<style>body{margin:0}div{width:100px;height:80px;border:4px solid transparent;background:linear-gradient(red,red) 100% 100%/20px 10px no-repeat}</style><div></div>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let (clip, tile) = output
        .items
        .iter()
        .find_map(|item| {
            if let DisplayItem::CssGradient {
                clip_rect,
                tile_rect,
                ..
            } = item
            {
                Some((*clip_rect, *tile_rect))
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!((clip.width, clip.height), (108.0, 88.0));
    assert_eq!(
        (tile.x, tile.y, tile.width, tile.height),
        (84.0, 74.0, 20.0, 10.0)
    );
}

#[test]
fn positioned_empty_pseudo_retains_all_corner_background_layers_under_zoom() {
    let page = Page::parse(
        r#"<style>
      body {margin:0}
      .panel {zoom:.691;position:relative;display:flex;flex-direction:column;width:420px;height:400px}
      .panel::before {content:"";position:absolute;inset:6px;--c:linear-gradient(#e2c07e,#e2c07e);
        background:var(--c) 0 0/26px 2px, var(--c) 0 0/2px 26px,
          var(--c) 100% 0/26px 2px, var(--c) 100% 0/2px 26px,
          var(--c) 0 100%/26px 2px, var(--c) 0 100%/2px 26px,
          var(--c) 100% 100%/26px 2px, var(--c) 100% 100%/2px 26px;
        background-repeat:no-repeat; border:1px solid #a07c4638}
      </style><div class=panel></div>"#,
        "https://example.test/",
    );
    let origin = page.dom.elements_named("div").next().unwrap();
    let styles = StyleSet::from_dom(&page.dom, &[], 800.0);
    let pseudo = styles
        .generated_pseudo(&origin, crate::engine::css::PseudoElement::Before)
        .unwrap();
    let pseudo_style = styles.get(&pseudo);
    assert_eq!(
        pseudo_style.background_image.0.len(),
        8,
        "{:?}",
        pseudo_style.background_layers
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let gradients = output
        .items
        .iter()
        .filter_map(|item| {
            if let DisplayItem::CssGradient {
                clip_rect,
                tile_rect,
                ..
            } = item
            {
                Some((*clip_rect, *tile_rect))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(gradients.len(), 8, "{:#?}", output.items);
    for (clip, tile) in gradients {
        assert!(clip.height > 250.0 && clip.width > 270.0, "{clip:?}");
        assert!((tile.width - 17.966).abs() < 0.01 || (tile.width - 1.382).abs() < 0.01);
        assert!((tile.height - 17.966).abs() < 0.01 || (tile.height - 1.382).abs() < 0.01);
    }
}

#[test]
fn propagated_body_background_images_paint_the_whole_canvas_not_the_body() {
    // CSS Backgrounds 3 §2.11.2: with a transparent root, the body background (color and
    // images) becomes the canvas background, positioned against the root element's box.
    let page = Page::parse(
        "<style>html{margin:0;padding:0}body{margin:0;height:30px;\
         background:green linear-gradient(red,blue) 5px 5px/10px 10px no-repeat}</style>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    assert_eq!(output.background, Color::rgb(0, 128, 0));
    let layers: Vec<_> = output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::CssGradient {
                clip_rect,
                tile_rect,
                ..
            } => Some((*clip_rect, *tile_rect)),
            _ => None,
        })
        .collect();
    let canvas = RectF {
        x: 0.0,
        y: 0.0,
        width: 800.0,
        height: 600.0,
    };
    let tile = RectF {
        x: 5.0,
        y: 5.0,
        width: 10.0,
        height: 10.0,
    };
    assert_eq!(layers, [(canvas, tile)]);
    assert!(!output.items.iter().any(|item| matches!(item,
        DisplayItem::SolidRect { color, .. } if *color == Color::rgb(0, 128, 0))));
}
