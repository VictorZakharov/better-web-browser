use super::*;

fn id(local: u128) -> NodeId {
    NodeId::from_wire((1_u128 << 64) | local).unwrap()
}

fn roots(range: std::ops::Range<u128>) -> RenderInvalidation {
    RenderInvalidation {
        roots: range.map(id).collect(),
        mutation_count: 1,
        ..RenderInvalidation::default()
    }
}

#[test]
fn deferred_checkpoints_keep_more_roots_before_collapsing_to_the_document() {
    let document = id(1);
    let count = MAX_INVALIDATION_ROOTS as u128;
    // Two checkpoints' worth of appended subtrees exceed the ordinary bound.
    let mut ordinary = roots(2..2 + count);
    ordinary.merge_conservatively(roots(2 + count..2 + 2 * count), document);
    assert_eq!(ordinary.roots, vec![document]);

    let mut deferred = roots(2..2 + count);
    deferred.merge_with_root_limit(
        roots(2 + count..2 + 2 * count),
        document,
        MAX_DEFERRED_INVALIDATION_ROOTS,
    );
    assert_eq!(deferred.roots.len(), 2 * MAX_INVALIDATION_ROOTS);

    deferred.merge_with_root_limit(
        roots(2 + 2 * count..3 + MAX_DEFERRED_INVALIDATION_ROOTS as u128),
        document,
        MAX_DEFERRED_INVALIDATION_ROOTS,
    );
    assert_eq!(deferred.roots, vec![document]);
}

/// An element root stays bounded only while no merged mutation made it a full or child-list
/// root, which already restyle the element itself.
#[test]
fn element_roots_yield_to_full_and_child_list_roots() {
    let document = id(1);
    let element = |node| RenderInvalidation {
        element_roots: vec![id(node)],
        ..roots(node..node + 1)
    };
    let mut merged = element(2);
    merged.merge_conservatively(element(3), document);
    merged.merge_conservatively(element(4), document);
    merged.merge_conservatively(roots(3..4), document);
    merged.merge_conservatively(
        RenderInvalidation {
            child_list_roots: vec![id(4)],
            ..roots(4..5)
        },
        document,
    );
    assert_eq!(merged.roots, vec![id(2), id(3), id(4)]);
    assert_eq!(merged.element_roots, vec![id(2)]);
    assert_eq!(merged.child_list_roots, vec![id(4)]);
}
