use super::*;
use crate::engine::affine::Affine2D;

fn rect() -> RectF {
    RectF {
        x: 0.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
    }
}
fn paint() -> DisplayItem {
    DisplayItem::SolidRect {
        rect: rect(),
        color: Default::default(),
        radius: 0.0,
    }
}

#[test]
fn invalid_transform_suppresses_the_whole_subtree_not_just_its_boundary() {
    for coefficient in [f64::NAN, f64::INFINITY, f64::MAX] {
        let layout = LayoutOutput {
            items: vec![
                paint(),
                DisplayItem::BeginTransform {
                    bounds: rect(),
                    matrix: Affine2D([coefficient, 0.0, 0.0, 1.0, 0.0, 0.0]),
                },
                paint(),
                DisplayItem::BeginTransform {
                    bounds: rect(),
                    matrix: Affine2D::IDENTITY,
                },
                paint(),
                DisplayItem::EndTransform { bounds: rect() },
                DisplayItem::EndTransform { bounds: rect() },
                paint(),
            ],
            ..Default::default()
        };
        let result = sanitize(layout);
        assert_eq!(result.items.len(), 2);
        assert!(
            result
                .items
                .iter()
                .all(|item| matches!(item, DisplayItem::SolidRect { .. }))
        );
    }
}

#[test]
fn singular_matrices_are_valid_css_values_and_remain_balanced_over_ipc() {
    let result = sanitize(LayoutOutput {
        items: vec![
            DisplayItem::BeginTransform {
                bounds: rect(),
                matrix: Affine2D([0.0; 6]),
            },
            paint(),
            DisplayItem::EndTransform { bounds: rect() },
        ],
        ..Default::default()
    });
    assert_eq!(result.items.len(), 3);
    let mut writer = crate::renderer_protocol::wire::WireWriter::new();
    assert!(super::super::layout::encode_layout(&mut writer, &result).is_ok());
}
