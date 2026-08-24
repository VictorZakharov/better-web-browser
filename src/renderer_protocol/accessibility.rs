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
            if removed.contains(&node.id) {
                return Err(ProtocolError::InvalidPayload(
                    "accessibility coalescing identity",
                ));
            }
            if next_added.contains(&node.id) {
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
