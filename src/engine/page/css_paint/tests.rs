use super::*;
use crate::engine::css::images::Gradient;

fn rect(width: f32, height: f32) -> RectF {
    RectF {
        x: 0.0,
        y: 0.0,
        width,
        height,
    }
}
fn gradient(value: &str, width: f32, height: f32) -> ResolvedGradient {
    Gradient::parse(value)
        .unwrap()
        .resolve(width, height, 16.0, 1.0, crate::engine::css::Color::BLACK)
        .unwrap()
}
fn pixel(image: &DecodedImage, x: usize, y: usize) -> &[u8] {
    let at = (y * image.width as usize + x) * 4;
    &image.bgra[at..at + 4]
}
fn item(width: f32, height: f32) -> DisplayItem {
    DisplayItem::CssGradient {
        clip_rect: rect(width, height),
        tile_rect: rect(width, height),
        gradient: gradient("linear-gradient(to right, red, blue)", width, height),
        radius: 0.0,
        repeat_x: true,
        repeat_y: true,
    }
}

#[test]
fn raster_keeps_premultiplied_alpha_and_endpoint_colors() {
    let box_rect = rect(100.0, 10.0);
    let image = raster(
        box_rect,
        box_rect,
        &gradient("linear-gradient(to right, red, blue)", 100.0, 10.0),
        0.0,
        false,
        false,
        100,
        10,
    )
    .unwrap();
    assert!(pixel(&image, 0, 5)[2] > 250 && pixel(&image, 99, 5)[0] > 250);
    let image = raster(
        box_rect,
        box_rect,
        &gradient("linear-gradient(transparent, #ff000080)", 100.0, 10.0),
        0.0,
        false,
        false,
        100,
        10,
    )
    .unwrap();
    for px in image.bgra.chunks_exact(4) {
        assert!(px[2] <= px[3]);
        assert_eq!((px[0], px[1]), (0, 0));
    }
}

#[test]
fn radius_no_repeat_and_tiled_offsets_preserve_transparent_pixels() {
    let clip = rect(20.0, 20.0);
    let tile = RectF {
        x: 5.0,
        y: 5.0,
        width: 10.0,
        height: 10.0,
    };
    let image = raster(
        clip,
        tile,
        &gradient("linear-gradient(red,red)", 10.0, 10.0),
        5.0,
        false,
        false,
        20,
        20,
    )
    .unwrap();
    assert_eq!(pixel(&image, 0, 0)[3], 0);
    assert_eq!(pixel(&image, 4, 10)[3], 0);
    assert_eq!(pixel(&image, 10, 10), &[0, 0, 255, 255]);
    assert_eq!(pixel(&image, 16, 10)[3], 0);
}

#[test]
fn radial_raster_uses_center_color_and_reaches_last_stop() {
    let clip = rect(101.0, 101.0);
    let image = raster(
        clip,
        clip,
        &gradient("radial-gradient(circle, red, blue 50%)", 101.0, 101.0),
        0.0,
        false,
        false,
        101,
        101,
    )
    .unwrap();
    assert_eq!(pixel(&image, 50, 50), &[0, 0, 255, 255]);
    assert!(pixel(&image, 0, 0)[0] > 250);
}

#[test]
fn cached_images_reuse_pixels_when_only_box_translation_changes() {
    let mut page = Page::parse("", "https://example.test/");
    let mut layout = LayoutOutput {
        items: vec![item(100.0, 20.0)],
        ..Default::default()
    };
    page.prepare_css_images(&mut layout);
    let DisplayItem::Image { url, .. } = &layout.items[0] else {
        panic!()
    };
    let pixels = page.images[url].bgra.clone();
    layout.items = vec![item(100.0, 20.0)];
    if let DisplayItem::CssGradient {
        clip_rect,
        tile_rect,
        ..
    } = &mut layout.items[0]
    {
        clip_rect.x += 40.0;
        clip_rect.y += 60.0;
        tile_rect.x += 40.0;
        tile_rect.y += 60.0;
    }
    page.prepare_css_images(&mut layout);
    let DisplayItem::Image { url, .. } = &layout.items[0] else {
        panic!()
    };
    assert!(Arc::ptr_eq(&pixels, &page.images[url].bgra));
    assert_eq!(page.images.len(), 1);
}

#[test]
fn repeated_preparation_of_a_realized_layout_preserves_its_image_resources() {
    let mut page = Page::parse("", "https://example.test/");
    let mut layout = LayoutOutput {
        items: vec![item(100.0, 20.0)],
        ..Default::default()
    };
    page.prepare_css_images(&mut layout);
    let DisplayItem::Image { url, .. } = &layout.items[0] else {
        panic!()
    };
    let key = url.clone();
    let pixels = page.images[&key].bgra.clone();
    page.prepare_css_images(&mut layout);
    assert!(Arc::ptr_eq(&pixels, &page.images[&key].bgra));
    assert!(matches!(&layout.items[0],DisplayItem::Image {url,..} if *url==key));
}

#[test]
fn resizing_retires_previous_generation_and_non_css_resources_survive() {
    let mut page = Page::parse("", "https://example.test/");
    page.images.insert(
        "https://example.test/icon.png".into(),
        DecodedImage {
            width: 1,
            height: 1,
            bgra: vec![0, 0, 0, 255].into(),
        },
    );
    let mut layout = LayoutOutput {
        items: vec![item(100.0, 20.0)],
        ..Default::default()
    };
    page.prepare_css_images(&mut layout);
    layout.items = vec![item(200.0, 20.0)];
    page.prepare_css_images(&mut layout);
    assert_eq!(page.images.len(), 2);
    layout.items.clear();
    page.prepare_css_images(&mut layout);
    assert_eq!(page.images.len(), 1);
    assert!(page.images.contains_key("https://example.test/icon.png"));
}

#[test]
fn excessive_dimensions_omit_paint_without_allocating_or_crossing_ipc() {
    let mut page = Page::parse("", "https://example.test/");
    let mut layout = LayoutOutput {
        items: vec![item(100_000.0, 100_000.0)],
        ..Default::default()
    };
    page.prepare_css_images(&mut layout);
    assert!(page.images.is_empty());
    assert!(matches!(
        layout.items[0],
        DisplayItem::SolidRect {
            color: crate::engine::css::Color::TRANSPARENT,
            ..
        }
    ));
}
