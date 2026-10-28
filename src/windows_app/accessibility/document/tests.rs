use super::*;
use better_web_browser::engine::RectF;
use better_web_browser::renderer_protocol::{SemanticActions, SemanticRole, SemanticSelection};

fn id(local: u64) -> DocumentNodeId {
    DocumentNodeId::new((1_u128 << 64) | u128::from(local)).unwrap()
}

fn node(local: u64, children: &[u64]) -> SemanticNode {
    SemanticNode {
        id: id(local),
        role: if local == 1 {
            SemanticRole::RootWebArea
        } else {
            SemanticRole::Button
        },
        name: format!("node {local}"),
        value: String::new(),
        description: String::new(),
        bounds: RectF::default(),
        children: children.iter().map(|child| id(*child)).collect(),
        level: None,
        disabled: false,
        read_only: false,
        actions: SemanticActions::default(),
        selection: None::<SemanticSelection>,
        checked: None,
    }
}

#[test]
fn full_tree_then_delta_updates_focus_and_removes_a_subtree() {
    let document = DocumentId::new(7).unwrap();
    let mut tree = AccessibilityDocument::default();
    tree.apply(
        document,
        1,
        AccessibilityUpdate {
            full: true,
            root: id(1),
            focus: id(1),
            nodes: vec![node(1, &[2]), node(2, &[])],
            added: Vec::new(),
            removed: Vec::new(),
        },
    )
    .unwrap();
    let platform = tree.platform_id(id(2)).unwrap();
    let root_platform = tree.platform_id(id(1)).unwrap();
    assert!(platform >= DOCUMENT_PLATFORM_ID_BASE);
    assert_eq!(tree.document_id_for_platform(platform), Some(id(2)));

    tree.apply(
        document,
        2,
        AccessibilityUpdate {
            full: false,
            root: id(1),
            focus: id(1),
            nodes: vec![node(1, &[])],
            added: Vec::new(),
            removed: vec![id(2)],
        },
    )
    .unwrap();
    assert!(tree.node(id(2)).is_none());
    assert_eq!(tree.focus(), Some(id(1)));
    assert_eq!(tree.platform_id(id(1)), Some(root_platform));
}

#[test]
fn unchanged_updates_stand_in_for_a_tree_until_semantics_are_enabled() {
    // Before an assistive technology is active the renderer reports no semantics; the full
    // tree it sends once enabled is then the document's first.
    let document = DocumentId::new(9).unwrap();
    let mut tree = AccessibilityDocument::default();
    for revision in [1, 2] {
        let applied = tree
            .apply(
                document,
                revision,
                AccessibilityUpdate::unchanged(id(1), id(1)),
            )
            .unwrap();
        assert!(applied.changed.is_empty() && !applied.full);
    }
    let full = AccessibilityUpdate {
        full: true,
        root: id(1),
        focus: id(1),
        nodes: vec![node(1, &[])],
        added: Vec::new(),
        removed: Vec::new(),
    };
    assert!(tree.apply(document, 3, full).unwrap().full);
    assert!(tree.node(id(1)).is_some());

    // A new document's placeholder retires the previous document's tree.
    let next = DocumentId::new(10).unwrap();
    let applied = tree
        .apply(next, 1, AccessibilityUpdate::unchanged(id(1), id(1)))
        .unwrap();
    assert!(applied.full);
    assert!(tree.node(id(1)).is_none());
}

#[test]
fn rejects_stale_cross_revision_and_disconnected_updates() {
    let document = DocumentId::new(8).unwrap();
    let mut tree = AccessibilityDocument::default();
    tree.apply(
        document,
        4,
        AccessibilityUpdate {
            full: true,
            root: id(1),
            focus: id(1),
            nodes: vec![node(1, &[])],
            added: Vec::new(),
            removed: Vec::new(),
        },
    )
    .unwrap();
    assert!(
        tree.apply(
            document,
            5,
            AccessibilityUpdate {
                full: true,
                root: id(1),
                focus: id(1),
                nodes: vec![node(1, &[])],
                added: Vec::new(),
                removed: Vec::new(),
            },
        )
        .is_err()
    );
    assert!(
        tree.apply(
            document,
            4,
            AccessibilityUpdate {
                full: false,
                root: id(1),
                focus: id(1),
                nodes: Vec::new(),
                added: Vec::new(),
                removed: Vec::new(),
            },
        )
        .is_err()
    );
    assert!(
        tree.apply(
            document,
            5,
            AccessibilityUpdate {
                full: false,
                root: id(1),
                focus: id(1),
                nodes: vec![node(2, &[])],
                added: Vec::new(),
                removed: Vec::new(),
            },
        )
        .is_err()
    );
}
