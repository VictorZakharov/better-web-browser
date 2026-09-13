use super::*;
use crate::engine::layout::StickyLayer;

fn layer() -> StickyLayer {
    StickyLayer {
        node_id: crate::engine::dom::NodeId::from_wire((1_u128 << 64) | 1).unwrap(),
        items: 0..1,
        normal: RectF {
            x: 0.0,
            y: 100.0,
            width: 100.0,
            height: 20.0,
        },
        containing: RectF {
            x: 0.0,
            y: 0.0,
            width: 500.0,
            height: 2000.0,
        },
        port: RectF {
            x: 0.0,
            y: 0.0,
            width: 500.0,
            height: 600.0,
        },
        insets: [Some(24.0), None, None, None],
        margins: [0.0; 4],
        parent: None,
        port_parent: None,
        viewport_port: true,
        offset: (0.0, 0.0),
    }
}

#[test]
fn retained_sticky_constraints_round_trip_and_reject_invalid_ranges_and_coordinates() {
    let mut presentation = sample();
    presentation.layout.sticky_layers = vec![layer()];
    let decoded = RendererPresentation::decode(&presentation.encode().unwrap()).unwrap();
    assert_eq!(
        decoded.layout.sticky_layers,
        presentation.layout.sticky_layers
    );
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        presentation.layout.sticky_layers[0] = layer();
        presentation.layout.sticky_layers[0].offset.1 = invalid;
        assert!(presentation.encode().is_err());
    }
    for invalid in [2..3, std::ops::Range { start: 1, end: 0 }] {
        presentation.layout.sticky_layers[0] = layer();
        presentation.layout.sticky_layers[0].items = invalid;
        assert!(presentation.encode().is_err());
    }
    for parent in [0, usize::MAX] {
        presentation.layout.sticky_layers[0] = layer();
        presentation.layout.sticky_layers[0].parent = Some(parent);
        assert!(presentation.encode().is_err());
    }
}

#[test]
fn invalid_page_derived_constraints_are_contained_before_wire_encoding() {
    for invalid in [f32::NAN, f32::INFINITY, f32::MAX] {
        let mut source = sample().layout.into_layout();
        source.sticky_layers = vec![layer()];
        source.sticky_layers[0].insets[0] = Some(invalid);
        let contained = PresentedLayout::from_layout(source);
        assert!(contained.sticky_layers.is_empty());
        assert!(!contained.items.is_empty());
    }
    let mut source = sample().layout.into_layout();
    source.sticky_layers = vec![layer()];
    source.sticky_layers[0].items = 0..usize::MAX;
    assert!(
        PresentedLayout::from_layout(source)
            .sticky_layers
            .is_empty()
    );
}
