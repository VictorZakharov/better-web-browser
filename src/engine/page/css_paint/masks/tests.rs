use super::*;
#[test]
fn source_over_alpha_is_color_independent_and_bounded() {
    assert_eq!(add(0, 128), 128);
    assert_eq!(add(128, 128), 192);
    assert_eq!(add(255, 0), 255);
    assert_eq!(add(255, 255), 255);
}
#[test]
fn empty_or_missing_layers_produce_transparent_masks() {
    let item = DisplayItem::BeginCssMask {
        bounds: RectF {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        },
        layers: vec![],
    };
    let image = raster(&item, 10, 10, &HashMap::new()).unwrap();
    assert!(image.bgra.iter().all(|v| *v == 0));
}

fn bounds() -> RectF {
    RectF {
        x: 0.0,
        y: 0.0,
        width: 2.0,
        height: 1.0,
    }
}
fn source(alpha: [u8; 2]) -> DecodedImage {
    DecodedImage {
        width: 2,
        height: 1,
        bgra: vec![0, 0, 0, alpha[0], 0, 0, 0, alpha[1]].into(),
    }
}
#[test]
fn url_masks_use_alpha_tile_geometry_and_resource_identity() {
    let tile = bounds();
    let item = DisplayItem::BeginCssMask {
        bounds: tile,
        layers: vec![DisplayItem::BackgroundImage {
            clip_rect: tile,
            tile_rect: tile,
            url: "mask.png".into(),
            repeat_x: false,
            repeat_y: false,
        }],
    };
    let mut images = HashMap::from([("mask.png".into(), source([0, 128]))]);
    let first = key(&item, &images).unwrap();
    assert_eq!(
        raster(&item, 2, 1, &images).unwrap().bgra.as_ref(),
        [0, 0, 0, 0, 128, 128, 128, 128]
    );
    images.insert("mask.png".into(), source([255, 255]));
    assert_ne!(first, key(&item, &images).unwrap());
    let mut moved = item.clone();
    crate::engine::layout::translate_display_items(std::slice::from_mut(&mut moved), 40.0, 30.0);
    assert_eq!(key(&item, &images), key(&moved, &images));
    let clip = RectF { width: 4.0, ..tile };
    assert_eq!(
        sample(&images["mask.png"], clip, tile, [3.0, 0.5], false, false),
        0
    );
    assert_eq!(
        sample(&images["mask.png"], clip, tile, [3.0, 0.5], true, false),
        255
    );
}
#[test]
fn gradients_mask_children_and_cache_repreparation_is_idempotent() {
    let mut page = Page::parse(
        "<style>body{margin:0}div{width:10px;height:10px;background:red;mask-image:linear-gradient(black 50%,transparent)}</style><div></div>",
        "https://example.test/",
    );
    let mut layout = crate::engine::layout::layout_page(
        &page,
        100.0,
        100.0,
        &mut crate::engine::layout::test_support::FixedMeasurer,
    );
    page.prepare_css_images(&mut layout);
    let url = layout
        .items
        .iter()
        .find_map(|i| match i {
            DisplayItem::BeginMask { url, .. } => Some(url.clone()),
            _ => None,
        })
        .unwrap();
    let pixels = page.images[&url].bgra.clone();
    assert_eq!(pixels[3], 255);
    assert!(pixels[(9 * 10) * 4 + 3] < 40);
    page.prepare_css_images(&mut layout);
    assert!(Arc::ptr_eq(&pixels, &page.images[&url].bgra));
}
