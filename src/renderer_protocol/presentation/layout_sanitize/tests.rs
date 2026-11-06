use super::*;
use crate::engine::css::Color;
use std::sync::Arc;

#[test]
fn page_derived_presentation_geometry_is_sanitized_before_ipc() {
    let layout = LayoutOutput {
        items: vec![
            DisplayItem::SolidRect {
                rect: RectF {
                    x: f32::INFINITY,
                    y: 0.0,
                    width: 10.0,
                    height: 10.0,
                },
                color: Color::default(),
                radius: 0.0,
            },
            DisplayItem::SolidRect {
                rect: RectF {
                    x: MAX_PRESENTATION_COORDINATE * 2.0,
                    y: -MAX_PRESENTATION_COORDINATE * 2.0,
                    width: -2.0,
                    height: MAX_PRESENTATION_COORDINATE * 2.0,
                },
                color: Color::default(),
                radius: f32::NAN,
            },
        ],
        content_height: f32::INFINITY,
        ..LayoutOutput::default()
    };

    let presented = sanitize(&layout);
    assert_eq!(presented.items.len(), 1);
    let DisplayItem::SolidRect { rect, radius, .. } = presented.items[0] else {
        panic!("expected retained rectangle");
    };
    assert_eq!(rect.x, MAX_PRESENTATION_COORDINATE);
    assert_eq!(rect.y, -MAX_PRESENTATION_COORDINATE);
    assert_eq!(rect.width, 0.0);
    assert_eq!(rect.height, MAX_PRESENTATION_COORDINATE);
    assert_eq!(radius, 0.0);
    assert_eq!(presented.content_height, 0.0);
}

#[test]
fn overlong_image_resource_key_does_not_invalidate_the_document() {
    let layout = LayoutOutput {
        items: vec![
            DisplayItem::Image {
                rect: RectF {
                    x: 0.0,
                    y: 0.0,
                    width: 20.0,
                    height: 20.0,
                },
                clip: None,
                url: format!("data:image/svg+xml,{}", "a".repeat(MAX_URL_BYTES)),
                alt: String::new(),
                tint: None,
            },
            DisplayItem::SolidRect {
                rect: RectF {
                    x: 20.0,
                    y: 0.0,
                    width: 20.0,
                    height: 20.0,
                },
                color: Color::default(),
                radius: 0.0,
            },
        ],
        ..LayoutOutput::default()
    };
    let presented = sanitize(&layout);
    assert_eq!(presented.items.len(), 1);
    assert!(matches!(presented.items[0], DisplayItem::SolidRect { .. }));
}

#[test]
fn glyphs_already_in_range_stay_shared() {
    let glyph = PositionedGlyph {
        raster_id: 1,
        x: 2.0,
        y: 3.0,
        width: 4.0,
        height: 5.0,
        color: false,
    };
    let in_range: Arc<[PositionedGlyph]> = Arc::from([glyph]);
    let out_of_range: Arc<[PositionedGlyph]> = Arc::from([
        glyph,
        PositionedGlyph {
            x: f32::NAN,
            ..glyph
        },
        PositionedGlyph {
            width: -1.0,
            ..glyph
        },
    ]);
    let run = |glyphs: &Arc<[PositionedGlyph]>| DisplayItem::Text {
        rect: RectF {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        },
        text: Arc::from("a"),
        font: FontSpec {
            family: "sans-serif".into(),
            size: 16.0,
            weight: 400,
            italic: false,
            underline: false,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            rtl: false,
            kerning: true,
            variants: Default::default(),
            features: Default::default(),
        },
        color: Color::default(),
        link: None,
        node_id: None,
        color_owner: None,
        raster_run_id: 0,
        glyphs: Arc::clone(glyphs),
    };
    let presented = sanitize(&LayoutOutput {
        items: vec![run(&in_range), run(&out_of_range)],
        ..LayoutOutput::default()
    });
    let glyphs = |index: usize| match &presented.items[index] {
        DisplayItem::Text { glyphs, .. } => Arc::clone(glyphs),
        item => panic!("expected a text run, got {item:?}"),
    };
    assert!(Arc::ptr_eq(&glyphs(0), &in_range));
    // A glyph without a finite position is dropped and a negative size becomes zero.
    assert_eq!(
        *glyphs(1),
        [
            glyph,
            PositionedGlyph {
                width: 0.0,
                ..glyph
            }
        ]
    );
}
