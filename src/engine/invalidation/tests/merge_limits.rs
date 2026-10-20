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
