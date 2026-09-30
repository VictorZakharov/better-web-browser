use super::*;

#[test]
fn native_bounds_index_has_linear_work_for_1k_2k_and_4k_plain_blocks() {
    for count in [1_000, 2_000, 4_000] {
        let dom = parse(&format!("<main>{}</main>", "<div></div>".repeat(count)));
        let blocks = dom.elements_named("div").collect::<Vec<_>>();
        let layout = LayoutOutput {
            node_paint_order: blocks.iter().map(|node| node.id()).collect(),
            node_bounds: blocks
                .iter()
                .enumerate()
                .map(|(index, node)| (node.id(), rect(0.0, index as f32 * 10.0, 100.0, 10.0)))
                .collect(),
            ..Default::default()
        };
        let mut index = CurrentNodes::new(&dom.document);
        assert_eq!(index.indexed_nodes, count + 5); // document/html/head/body/main
        assert_eq!(index.nodes.len(), index.indexed_nodes);
        assert_eq!(index.hit(&layout, 50.0, 5.0).unwrap().id(), blocks[0].id());
        assert_eq!(index.candidate_lookups, count);
        // Count operations rather than assert a wall-clock threshold on CI.
        assert!(index.indexed_nodes + index.candidate_lookups <= count * 2 + 5);
        assert_equivalent(
            &dom,
            &layout,
            &[(50.0, 5.0), (50.0, (count - 1) as f32 * 10.0 + 5.0)],
        );
    }
}

#[test]
fn native_bounds_index_keeps_reverse_order_inclusive_edges_and_empty_box_rejection() {
    let dom = parse("<div id=first></div><div id=last></div><div id=empty></div>");
    let first = element(&dom, "first");
    let last = element(&dom, "last");
    let empty = element(&dom, "empty");
    let layout = layout_for(&[
        (&first, rect(0.0, 0.0, 100.0, 100.0)),
        (&last, rect(20.0, 20.0, 50.0, 50.0)),
        (&empty, rect(20.0, 20.0, 0.0, 0.0)),
    ]);
    assert_equivalent(
        &dom,
        &layout,
        &[
            (0.0, 0.0),
            (20.0, 20.0),
            (70.0, 70.0),
            (100.0, 100.0),
            (101.0, 50.0),
        ],
    );
    assert_eq!(
        node_at_point(&dom, &layout, 70.0, 70.0).unwrap().id(),
        last.id()
    );
    assert!(node_at_point(&dom, &layout, 101.0, 50.0).is_none());
}
