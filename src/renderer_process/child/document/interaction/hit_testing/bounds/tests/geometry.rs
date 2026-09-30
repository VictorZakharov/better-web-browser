use super::*;
use crate::engine::layout::ScrollBox;

#[test]
fn native_bounds_index_preserves_nested_scroll_clips_clip_paths_and_pointer_events() {
    let dom = parse(
        "<main id=pane><div id=first></div><div id=second></div></main><aside id=outside></aside>",
    );
    let pane = element(&dom, "pane");
    let first = element(&dom, "first");
    let second = element(&dom, "second");
    let outside = element(&dom, "outside");
    let mut layout = layout_for(&[
        (&outside, rect(0.0, 0.0, 200.0, 200.0)),
        (&pane, rect(0.0, 0.0, 100.0, 100.0)),
        (&first, rect(0.0, 0.0, 100.0, 100.0)),
        (&second, rect(0.0, 100.0, 100.0, 100.0)),
    ]);
    layout.scroll_boxes.insert(
        pane.id(),
        ScrollBox {
            port: rect(0.0, 0.0, 100.0, 100.0),
            offset_y: 75.0,
            clip_x: true,
            clip_y: true,
            ..Default::default()
        },
    );
    assert_equivalent(&dom, &layout, &[(50.0, 10.0), (50.0, 50.0), (50.0, 120.0)]);
    assert_eq!(
        node_at_point(&dom, &layout, 50.0, 50.0).unwrap().id(),
        second.id()
    );
    assert_eq!(
        node_at_point(&dom, &layout, 50.0, 120.0).unwrap().id(),
        outside.id()
    );
    layout
        .clip_paths
        .insert(pane.id(), rect(20.0, 20.0, 60.0, 60.0));
    assert_eq!(
        node_at_point(&dom, &layout, 10.0, 50.0).unwrap().id(),
        outside.id()
    );
    layout.hit_excluded.insert(pane.id());
    assert_eq!(
        node_at_point(&dom, &layout, 50.0, 50.0).unwrap().id(),
        second.id()
    );
    layout.hit_excluded.insert(second.id());
    assert_eq!(
        node_at_point(&dom, &layout, 50.0, 50.0).unwrap().id(),
        outside.id()
    );
    assert_equivalent(&dom, &layout, &[(10.0, 50.0), (50.0, 50.0), (50.0, 120.0)]);
}

#[test]
fn native_bounds_index_reads_current_sticky_translation_without_dom_mutation() {
    let dom = parse("<main id=parent><aside id=sticky></aside></main>");
    let parent = element(&dom, "parent");
    let sticky = element(&dom, "sticky");
    let mut layout = layout_for(&[
        (&parent, rect(0.0, 0.0, 100.0, 500.0)),
        (&sticky, rect(0.0, 100.0, 100.0, 50.0)),
    ]);
    let version = dom.document.subtree_mutation_version();
    for offset in [0.0, 75.0, 200.0] {
        layout.sticky_offsets.insert(sticky.id(), (0.0, offset));
        let y = 110.0 + offset;
        assert_eq!(
            node_at_point(&dom, &layout, 50.0, y).unwrap().id(),
            sticky.id()
        );
        assert_equivalent(&dom, &layout, &[(50.0, y), (50.0, 90.0), (50.0, y + 51.0)]);
        assert_eq!(dom.document.subtree_mutation_version(), version);
    }
}

#[test]
fn native_bounds_index_keeps_top_layer_ancestor_scroll_and_clip_escape() {
    let dom = parse("<main id=pane><aside id=popover popover></aside></main>");
    let pane = element(&dom, "pane");
    let popover = element(&dom, "popover");
    popover.set_popover_order(1);
    let mut layout = layout_for(&[
        (&pane, rect(0.0, 0.0, 10.0, 10.0)),
        (&popover, rect(100.0, 100.0, 100.0, 100.0)),
    ]);
    layout.scroll_boxes.insert(
        pane.id(),
        ScrollBox {
            port: rect(0.0, 0.0, 10.0, 10.0),
            offset_y: 75.0,
            clip_x: true,
            clip_y: true,
            ..Default::default()
        },
    );
    layout
        .clip_paths
        .insert(pane.id(), rect(0.0, 0.0, 10.0, 10.0));
    assert_eq!(
        node_at_point(&dom, &layout, 150.0, 150.0).unwrap().id(),
        popover.id()
    );
    assert_equivalent(
        &dom,
        &layout,
        &[(150.0, 150.0), (100.0, 100.0), (200.0, 200.0)],
    );
}
