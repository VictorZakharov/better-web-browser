use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

fn layout(markup: &str) -> (Page, LayoutOutput) {
    let page = Page::parse(
        &format!("<style>body{{margin:0}}</style>{markup}"),
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    (page, output)
}
fn node(page: &Page, id: &str) -> NodeRef {
    Node::descendants(&page.dom.document)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap()
}

#[test]
fn scale_x_progress_changes_visual_width_without_shrinking_layout_or_resize_observers() {
    for progress in [0.0, 0.04, 0.25, 0.5, 1.0] {
        let (page, output) = layout(&format!(
            "<div id=fill style='width:200px;height:8px;background:red;transform-origin:left;transform:scaleX({progress})'></div><div id=next style='height:20px'></div>"
        ));
        let fill = node(&page, "fill");
        let next = node(&page, "next");
        let rect = output.node_bounds[&fill.id()];
        assert_eq!(rect.x, 0.0);
        assert_eq!(rect.width, 200.0 * progress);
        assert_eq!(rect.height, 8.0);
        assert_eq!(output.node_bounds[&next.id()].y, 8.0);
        assert_eq!(output.resize_boxes[&fill.id()].border_width, 200.0);
        let geometry =
            layout_geometry_with_style_viewport(&page, 800.0, 600.0, 800.0, &mut FixedMeasurer);
        assert_eq!(geometry.node_bounds, output.node_bounds);
    }
}

#[test]
fn nested_inverse_rotations_keep_exact_corners_instead_of_retransforming_bounding_boxes() {
    let (page, output) = layout(
        "<main id=outer style='width:100px;height:100px;transform-origin:0 0;transform:rotate(45deg)'><div id=inner style='width:40px;height:20px;transform-origin:0 0;transform:rotate(-45deg);background:red'></div></main>",
    );
    let inner = node(&page, "inner");
    let rect = output.node_bounds[&inner.id()];
    assert!(rect.x.abs() < 0.00001 && rect.y.abs() < 0.00001, "{rect:?}");
    assert!(
        (rect.width - 40.0).abs() < 0.00001 && (rect.height - 20.0).abs() < 0.00001,
        "{rect:?}"
    );
}

#[test]
fn rotated_hit_testing_rejects_the_empty_corners_of_the_axis_aligned_bounding_box() {
    let (page, output) = layout(
        "<div id=rotated style='position:absolute;left:100px;top:100px;width:100px;height:100px;transform:rotate(45deg);background:red'></div>",
    );
    let target = node(&page, "rotated");
    let snapshot = HitTestSnapshot::from_layout(&output, &page.dom.document);
    assert!(
        snapshot
            .elements_at(150.0, 150.0)
            .iter()
            .any(|n| n.id() == target.id())
    );
    let bounds = output.node_bounds[&target.id()];
    assert!(
        !snapshot
            .elements_at(bounds.x + 1.0, bounds.y + 1.0)
            .iter()
            .any(|n| n.id() == target.id())
    );
}

#[test]
fn transformed_inline_images_share_geometry_and_reference_origins_with_blocks() {
    let (page, output) = layout(
        "<img id=image width=100 height=40 src='data:image/png;base64,broken' style='transform:scaleX(.5);transform-origin:left'>",
    );
    let image = node(&page, "image");
    assert_eq!(output.node_bounds[&image.id()].width, 50.0);
    assert_eq!(output.resize_boxes[&image.id()].border_width, 100.0);
    assert!(
        output
            .items
            .iter()
            .any(|i| matches!(i, DisplayItem::BeginTransform { .. }))
    );
}

#[test]
fn scrolling_displacements_follow_the_containers_affine_axes_not_the_childs() {
    for (transform, expected) in [("rotate(90deg)", (30.0, 0.0)), ("scale(2)", (0.0, -60.0))] {
        let (page, before) = layout(&format!(
            "<div id=scroll style='position:absolute;left:100px;top:100px;width:100px;height:100px;overflow:auto;transform-origin:0 0;transform:{transform}'><div id=child style='margin-top:70px;width:20px;height:20px;transform:rotate(20deg);background:red'></div><div style='height:200px'></div></div>"
        ));
        let scroll = node(&page, "scroll");
        let child = node(&page, "child");
        let unscrolled = before.visual_rect(&child).unwrap();
        scroll.scroll_offset.set((0.0, 30.0));
        let after = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        assert_eq!(after.scroll_boxes[&scroll.id()].offset_y, 30.0);
        // The 15px classic scrollbar consumes layout width, not scaled width.
        assert_eq!(after.scroll_boxes[&scroll.id()].port.width, 85.0);
        let moved = after.visual_rect(&child).unwrap();
        assert!(
            (moved.x - unscrolled.x - expected.0).abs() < 0.001,
            "{transform}: {unscrolled:?} -> {moved:?}"
        );
        assert!(
            (moved.y - unscrolled.y - expected.1).abs() < 0.001,
            "{transform}: {unscrolled:?} -> {moved:?}"
        );
        let snapshot = HitTestSnapshot::from_layout(&after, &page.dom.document);
        assert!(
            snapshot
                .elements_at(moved.x + moved.width / 2.0, moved.y + moved.height / 2.0)
                .iter()
                .any(|n| n.id() == child.id())
        );
    }
}
