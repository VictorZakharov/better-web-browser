use super::*;
use crate::engine::DecodedImage;
use crate::renderer_protocol::presentation::tests::sample;
use crate::renderer_protocol::{PresentedGlyphRaster, PresentedImage};
use std::sync::Arc;

fn frame(revision: u64) -> RendererPresentation {
    let mut value = sample();
    value.revision = revision;
    value.glyphs.clear();
    value
}

fn image(url: impl Into<String>, value: u8) -> PresentedImage {
    PresentedImage {
        url: url.into(),
        image: DecodedImage {
            width: 1,
            height: 1,
            bgra: vec![value; 4].into(),
        },
    }
}

fn merged(first: RendererPresentation, next: RendererPresentation) -> RendererPresentation {
    let (value, remaining) = first.coalesce(next).unwrap();
    assert!(remaining.is_none());
    value
}

#[test]
fn repeated_canvas_updates_keep_only_the_latest_bitmap_and_release_old_pixels() {
    let mut retained = frame(1);
    retained.images.push(image("canvas:1", 1));
    let first_pixels = Arc::downgrade(&retained.images[0].image.bgra);
    for revision in 2..100 {
        let mut next = frame(revision);
        next.images.push(image("canvas:1", revision as u8));
        retained = merged(retained, next);
        assert_eq!(retained.images.len(), 1);
        assert_eq!(retained.images[0].image.bgra.as_ref(), &[revision as u8; 4]);
    }
    assert!(
        first_pixels.upgrade().is_none(),
        "superseded pixels are still retained"
    );
}

#[test]
fn image_updates_and_retirements_apply_in_original_revision_order() {
    let mut first = frame(1);
    first.images = vec![image("a", 1), image("b", 2), image("c", 3)];
    first.retired_image_keys = vec!["d".into()];
    let mut next = frame(2);
    next.images = vec![image("a", 4), image("d", 5), image("a", 6)];
    next.retired_image_keys = vec!["b".into(), "e".into()];
    let combined = merged(first, next);
    assert_eq!(combined.retired_image_keys, ["b", "e"]);
    assert_eq!(
        combined
            .images
            .iter()
            .map(|image| (image.url.as_str(), image.image.bgra[0]))
            .collect::<Vec<_>>(),
        [("c", 3), ("d", 5), ("a", 6)]
    );
    assert!(combined.encode().is_ok());
}

#[test]
fn same_epoch_glyphs_deduplicate_by_id_and_new_epochs_drop_old_rasters() {
    let glyph = |id, value| PresentedGlyphRaster {
        id,
        color: false,
        image: image("", value).image,
    };
    let mut first = frame(1);
    first.glyphs = vec![glyph(1, 1), glyph(2, 2)];
    let mut next = frame(2);
    next.glyphs = vec![glyph(1, 3)];
    let combined = merged(first, next);
    assert_eq!(
        combined
            .glyphs
            .iter()
            .map(|glyph| (glyph.id, glyph.image.bgra[0]))
            .collect::<Vec<_>>(),
        [(2, 2), (1, 3)]
    );
    assert!(combined.encode().is_ok());
    let mut reset = frame(3);
    reset.glyph_epoch += 1;
    reset.glyphs.push(glyph(1, 4));
    let reset = merged(combined, reset);
    assert_eq!(reset.glyphs.len(), 1);
    assert_eq!(reset.glyphs[0].image.bgra[0], 4);
}

#[test]
fn image_and_retirement_count_overflow_preserves_both_originals() {
    for retirement in [false, true] {
        let mut first = frame(1);
        let mut next = frame(2);
        if retirement {
            first.retired_image_keys = (0..MAX_PRESENTED_IMAGES)
                .map(|id| format!("canvas:{id}"))
                .collect();
            next.retired_image_keys = vec!["canvas:next".into()];
        } else {
            first.images = (0..MAX_PRESENTED_IMAGES)
                .map(|id| image(format!("canvas:{id}"), 1))
                .collect();
            next.images = vec![image("canvas:next", 2)];
        }
        let first_expected = first.encode().unwrap();
        let next_expected = next.encode().unwrap();
        let (first, next) = first.coalesce(next).unwrap();
        assert_eq!(first.encode().unwrap(), first_expected);
        assert_eq!(next.unwrap().encode().unwrap(), next_expected);
    }
}

#[test]
fn image_byte_overflow_splits_valid_presentations_without_dropping_updates() {
    let pixels: Arc<[u8]> = vec![255; 1024 * 1024].into();
    let images = |start| {
        (start..start + 33)
            .map(|id| PresentedImage {
                url: format!("canvas:{id}"),
                image: DecodedImage {
                    width: 512,
                    height: 512,
                    bgra: pixels.clone(),
                },
            })
            .collect()
    };
    let mut first = frame(1);
    first.images = images(0);
    let mut next = frame(2);
    next.images = images(33);
    assert!(first.one_shot_resource_bytes() < MAX_RENDERER_PRESENTATION_BYTES);
    assert!(next.one_shot_resource_bytes() < MAX_RENDERER_PRESENTATION_BYTES);
    let (first, next) = first.coalesce(next).unwrap();
    assert_eq!(first.images.len(), 33);
    assert_eq!(next.unwrap().images.len(), 33);
}

#[test]
fn glyph_count_and_byte_overflow_split_without_dropping_rasters() {
    let mut first = frame(1);
    first.glyphs = (1..=MAX_GLYPH_RASTERS as u32)
        .map(|id| PresentedGlyphRaster {
            id,
            color: false,
            image: image("", 1).image,
        })
        .collect();
    let mut next = frame(2);
    next.glyphs.push(PresentedGlyphRaster {
        id: MAX_GLYPH_RASTERS as u32 + 1,
        color: false,
        image: image("", 2).image,
    });
    assert!(first.coalesce(next).unwrap().1.is_some());

    let pixels: Arc<[u8]> = vec![255; 4 * 1024 * 1024].into();
    let rasters = |start| {
        (start..start + 7)
            .map(|id| PresentedGlyphRaster {
                id,
                color: false,
                image: DecodedImage {
                    width: 1024,
                    height: 1024,
                    bgra: pixels.clone(),
                },
            })
            .collect()
    };
    let mut first = frame(1);
    first.glyphs = rasters(1);
    let mut next = frame(2);
    next.glyphs = rasters(8);
    let (first, next) = first.coalesce(next).unwrap();
    assert_eq!(first.glyphs.len(), 7);
    assert_eq!(next.unwrap().glyphs.len(), 7);
}
