use super::*;

fn raster(id: u32, color: bool) -> PresentedGlyphRaster {
    PresentedGlyphRaster {
        id,
        image: DecodedImage {
            width: 1,
            height: 1,
            bgra: vec![255; 4],
        },
        color,
    }
}

fn glyph(id: u32, color: bool) -> PositionedGlyph {
    PositionedGlyph {
        raster_id: id,
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
        color,
    }
}

#[test]
fn exact_pixel_dimensions_survive_standard_and_fractional_dpi() {
    let run = GlyphRunBitmap {
        bitmap: null_mut(),
        offset_x: -2,
        offset_y: 3,
        source_width: 17,
        source_height: 19,
    };
    let text_rect = RectF {
        x: 10.2,
        y: 5.2,
        width: 100.0,
        height: 40.0,
    };

    let standard = run.destination_rect(text_rect, 7, 80, 1.0);
    assert_eq!((standard.left, standard.top), (8, 81));
    assert_eq!((standard.width(), standard.height()), (17, 19));

    let fractional = run.destination_rect(text_rect, 7, 80, 1.25);
    assert_eq!((fractional.left, fractional.top), (11, 83));
    assert_eq!((fractional.width(), fractional.height()), (17, 19));
}

#[test]
fn source_over_composites_premultiplied_pixels() {
    let mut destination = [0, 0, 128, 128];
    source_over(&mut destination, [0, 128, 0, 128]);

    assert_eq!(destination, [0, 128, 63, 191]);
}

#[test]
fn tint_pixel_preserves_premultiplied_alpha() {
    assert_eq!(tint_pixel(128, [10, 20, 30, 128]), [7, 5, 2, 64]);
}

#[test]
fn unknown_or_mismatched_glyph_resources_fail_closed() {
    let resources = HashMap::from([(1, raster(1, false))]);

    assert!(pixel_bounds(&[glyph(2, false)], &resources, 1.0).is_none());
    assert!(pixel_bounds(&[glyph(1, true)], &resources, 1.0).is_none());
    assert!(pixel_bounds(&[glyph(1, false)], &resources, 1.0).is_some());
}
