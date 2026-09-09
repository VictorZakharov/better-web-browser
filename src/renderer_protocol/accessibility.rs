//! Bounded renderer-owned accessibility semantics carried with a presentation revision.

mod codec;

use crate::engine::RectF;

use super::{DocumentNodeId, ProtocolError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SemanticRole {
    RootWebArea,
    TextRun,
    Paragraph,
    Heading,
    Link,
    Button,
    TextInput,
    MultilineTextInput,
    PasswordInput,
    SearchInput,
    ComboBox,
    List,
    ListItem,
    Table,
    RowGroup,
    Row,
    Cell,
    RowHeader,
    ColumnHeader,
    Image,
    Form,
    Main,
    Navigation,
    Header,
    Footer,
    Article,
    Section,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SemanticActions {
    pub focus: bool,
    pub invoke: bool,
    pub set_value: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SemanticSelection {
    /// UTF-16 offsets, matching native Windows edit controls and renderer text input.
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticNode {
    pub id: DocumentNodeId,
    pub role: SemanticRole,
    pub name: String,
    pub value: String,
    pub description: String,
    /// Renderer document coordinates. The browser applies DPI, scroll, and chrome offsets.
    pub bounds: RectF,
    pub children: Vec<DocumentNodeId>,
    pub level: Option<u32>,
    pub disabled: bool,
    pub read_only: bool,
    pub actions: SemanticActions,
    pub selection: Option<SemanticSelection>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AccessibilityUpdate {
    /// The first update for a document is a full tree; later updates contain changed nodes only.
    pub full: bool,
    pub root: DocumentNodeId,
    pub focus: DocumentNodeId,
    pub nodes: Vec<SemanticNode>,
    /// Nodes introduced by this delta. Empty for a full bootstrap.
    pub added: Vec<DocumentNodeId>,
    pub removed: Vec<DocumentNodeId>,
}

impl AccessibilityUpdate {
    pub fn full_root(root: DocumentNodeId, name: impl Into<String>, bounds: RectF) -> Self {
        Self {
            full: true,
            root,
            focus: root,
            nodes: vec![SemanticNode {
                id: root,
                role: SemanticRole::RootWebArea,
                name: name.into(),
                value: String::new(),
                description: String::new(),
                bounds,
                children: Vec::new(),
                level: None,
                disabled: false,
                read_only: false,
                actions: SemanticActions::default(),
                selection: None,
            }],
            added: Vec::new(),
            removed: Vec::new(),
        }
    }

    pub(crate) fn coalesce(self, next: Self) -> Result<Self, ProtocolError> {
        if next.full {
            return Ok(next);
        }
        if self.root != next.root {
            return Err(ProtocolError::InvalidPayload(
                "accessibility coalescing root",
            ));
        }

        let mut nodes = self
            .nodes
            .into_iter()
            .map(|node| (node.id, node))
            .collect::<std::collections::HashMap<_, _>>();
        if self.full {
            for removed in next.removed {
                if nodes.remove(&removed).is_none() {
                    return Err(ProtocolError::InvalidPayload(
                        "accessibility coalescing removal",
                    ));
                }
            }
            for node in next.nodes {
                nodes.insert(node.id, node);
            }
            let mut nodes = nodes.into_values().collect::<Vec<_>>();
            nodes.sort_by_key(|node| node.id.get());
            return Ok(Self {
                full: true,
                root: next.root,
                focus: next.focus,
                nodes,
                added: Vec::new(),
                removed: Vec::new(),
            });
        }

        let next_added = next
            .added
            .into_iter()
            .collect::<std::collections::HashSet<_>>();
        let mut added = self
            .added
            .into_iter()
            .collect::<std::collections::HashSet<_>>();
        let mut removed = self
            .removed
            .into_iter()
            .collect::<std::collections::HashSet<_>>();
        for id in next.removed {
            nodes.remove(&id);
            if !added.remove(&id) {
                removed.insert(id);
            }
        }
        for node in next.nodes {
            let reintroduced = next_added.contains(&node.id);
            if removed.contains(&node.id) && !reintroduced {
                return Err(ProtocolError::InvalidPayload(
                    "accessibility coalescing identity",
                ));
            }
            // A stable DOM node can leave the semantic tree (hidden, detached or
            // outside a fullscreen subtree) and return before either delta is
            // delivered. Relative to the consumer it is an update, not a new ID.
            if !removed.remove(&node.id) && reintroduced {
                added.insert(node.id);
            }
            nodes.insert(node.id, node);
        }
        let mut nodes = nodes.into_values().collect::<Vec<_>>();
        nodes.sort_by_key(|node| node.id.get());
        let mut added = added.into_iter().collect::<Vec<_>>();
        added.sort_by_key(|id| id.get());
        let mut removed = removed.into_iter().collect::<Vec<_>>();
        removed.sort_by_key(|id| id.get());
        Ok(Self {
            full: false,
            root: next.root,
            focus: next.focus,
            nodes,
            added,
            removed,
        })
    }

    pub(in crate::renderer_protocol) fn encode_into(
        &self,
        writer: &mut super::wire::WireWriter,
    ) -> Result<(), ProtocolError> {
        codec::encode(writer, self)
    }

    pub(in crate::renderer_protocol) fn decode_from(
        reader: &mut super::wire::WireReader<'_>,
    ) -> Result<Self, ProtocolError> {
        codec::decode(reader)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RectF;

    fn id(local: u64) -> DocumentNodeId {
        DocumentNodeId::new((1_u128 << 64) | u128::from(local)).unwrap()
    }

    fn node(local: u64, children: Vec<DocumentNodeId>) -> SemanticNode {
        SemanticNode {
            id: id(local),
            role: if local == 1 {
                SemanticRole::RootWebArea
            } else {
                SemanticRole::Paragraph
            },
            name: format!("node {local}"),
            value: String::new(),
            description: String::new(),
            bounds: RectF::default(),
            children,
            level: None,
            disabled: false,
            read_only: false,
            actions: SemanticActions::default(),
            selection: None,
        }
    }

    #[test]
    fn coalescing_restored_semantics_updates_an_existing_consumer_identity() {
        let hidden = visibility_delta(1, 0);
        let shown = visibility_delta(0, 1);
        let combined = hidden.coalesce(shown).unwrap();
        assert!(combined.added.is_empty());
        assert!(combined.removed.is_empty());
        assert_eq!(
            combined.nodes,
            vec![node(1, vec![id(2)]), node(2, Vec::new())]
        );
    }

    #[test]
    fn coalescing_does_not_accept_an_undeclared_restored_identity() {
        let hidden = visibility_delta(1, 0);
        let mut invalid = visibility_delta(0, 1);
        invalid.added.clear();
        assert!(hidden.coalesce(invalid).is_err());
    }

    #[test]
    fn coalesced_visibility_sequences_match_their_net_tree_membership() {
        // Exhaust all initial states and four updates of two independent nodes:
        // 1,024 sequences, including newly added/removed/restored nodes and IDs
        // that already existed when the consumer last saw the tree.
        for encoded in 0..1024_u32 {
            let states = (0..5)
                .map(|i| ((encoded >> (i * 2)) & 3) as u8)
                .collect::<Vec<_>>();
            let mut combined = visibility_delta(states[0], states[1]);
            for pair in states[1..].windows(2) {
                combined = combined
                    .coalesce(visibility_delta(pair[0], pair[1]))
                    .unwrap();
            }
            let original = visible_ids(states[0]);
            let final_ids = visible_ids(states[4]);
            assert_eq!(
                combined.added,
                final_ids
                    .iter()
                    .filter(|id| !original.contains(id))
                    .copied()
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                combined.removed,
                original
                    .iter()
                    .filter(|id| !final_ids.contains(id))
                    .copied()
                    .collect::<Vec<_>>()
            );
            let mut actual = std::collections::HashMap::from([(id(1), node(1, original.clone()))]);
            for id in &original {
                actual.insert(*id, node(id.get() as u64, Vec::new()));
            }
            for id in &combined.removed {
                assert!(actual.remove(id).is_some());
            }
            for item in combined.nodes {
                actual.insert(item.id, item);
            }
            assert_eq!(actual.len(), final_ids.len() + 1);
            assert_eq!(actual[&id(1)].children, final_ids);
            for id in &final_ids {
                assert!(actual.contains_key(id));
            }
        }
    }

    fn visible_ids(mask: u8) -> Vec<DocumentNodeId> {
        (0..2)
            .filter(|bit| mask & (1 << bit) != 0)
            .map(|bit| id(2 + bit))
            .collect()
    }

    fn visibility_delta(before: u8, after: u8) -> AccessibilityUpdate {
        let previous = visible_ids(before);
        let next = visible_ids(after);
        let mut nodes = vec![node(1, next.clone())];
        nodes.extend(next.iter().map(|id| node(id.get() as u64, Vec::new())));
        AccessibilityUpdate {
            full: false,
            root: id(1),
            focus: id(1),
            nodes,
            added: next
                .iter()
                .filter(|id| !previous.contains(id))
                .copied()
                .collect(),
            removed: previous
                .iter()
                .filter(|id| !next.contains(id))
                .copied()
                .collect(),
        }
    }

    #[test]
    fn coalescing_deltas_cancels_a_new_node_removed_before_delivery() {
        let first = AccessibilityUpdate {
            full: false,
            root: id(1),
            focus: id(1),
            nodes: vec![node(1, vec![id(2)]), node(2, Vec::new())],
            added: vec![id(2)],
            removed: Vec::new(),
        };
        let second = AccessibilityUpdate {
            full: false,
            root: id(1),
            focus: id(1),
            nodes: vec![node(1, Vec::new())],
            added: Vec::new(),
            removed: vec![id(2)],
        };

        let combined = first.coalesce(second).unwrap();
        assert!(!combined.full);
        assert_eq!(combined.nodes.len(), 1);
        assert!(combined.added.is_empty());
        assert!(combined.removed.is_empty());
    }

    #[test]
    fn coalescing_a_delta_into_bootstrap_keeps_a_complete_tree() {
        let bootstrap = AccessibilityUpdate::full_root(id(1), "root", RectF::default());
        let delta = AccessibilityUpdate {
            full: false,
            root: id(1),
            focus: id(2),
            nodes: vec![node(1, vec![id(2)]), node(2, Vec::new())],
            added: vec![id(2)],
            removed: Vec::new(),
        };

        let combined = bootstrap.coalesce(delta).unwrap();
        assert!(combined.full);
        assert_eq!(combined.focus, id(2));
        assert_eq!(combined.nodes.len(), 2);
        assert!(combined.added.is_empty());
    }
}
