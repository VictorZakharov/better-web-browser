use super::*;

#[test]
fn local_removal_evidence_survives_additions_but_not_unknown_removals() {
    let document = NodeId::from_wire((1_u128 << 64) | 1).unwrap();
    let child = NodeId::from_wire((1_u128 << 64) | 2).unwrap();
    let addition = RenderInvalidation {
        roots: vec![document],
        ..RenderInvalidation::default()
    };
    let local = RenderInvalidation {
        roots: vec![document],
        removed_nodes: vec![child],
        removals_are_local: true,
        attribute_changes: Vec::new(),
        ..RenderInvalidation::default()
    };
    let mut combined = addition.clone();
    combined.merge_conservatively(local, document);
    assert!(combined.removals_are_local);
    combined.merge_conservatively(addition, document);
    assert!(combined.removals_are_local);
    combined.merge_conservatively(
        RenderInvalidation {
            roots: vec![document],
            removed_nodes: vec![child],
            ..RenderInvalidation::default()
        },
        document,
    );
    assert!(!combined.removals_are_local);
}
