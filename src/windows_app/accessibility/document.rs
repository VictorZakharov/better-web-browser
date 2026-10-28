//! Transactional browser-side validation of renderer semantic deltas.

use better_web_browser::limits::{MAX_ACCESSIBILITY_EDGES, MAX_ACCESSIBILITY_NODES};
use better_web_browser::renderer_protocol::{
    AccessibilityUpdate, DocumentId, DocumentNodeId, SemanticNode,
};
use std::collections::{HashMap, HashSet, hash_map::Entry};

pub(super) const DOCUMENT_PLATFORM_ID_BASE: u64 = 1 << 32;

#[derive(Clone)]
pub(in crate::windows_app) struct AccessibilityDocument {
    document: Option<DocumentId>,
    revision: u64,
    root: Option<DocumentNodeId>,
    focus: Option<DocumentNodeId>,
    nodes: HashMap<DocumentNodeId, SemanticNode>,
    platform_ids: HashMap<DocumentNodeId, u64>,
    document_ids: HashMap<u64, DocumentNodeId>,
    next_platform_id: u64,
}

impl Default for AccessibilityDocument {
    fn default() -> Self {
        Self {
            document: None,
            revision: 0,
            root: None,
            focus: None,
            nodes: HashMap::new(),
            platform_ids: HashMap::new(),
            document_ids: HashMap::new(),
            next_platform_id: DOCUMENT_PLATFORM_ID_BASE,
        }
    }
}

pub(in crate::windows_app) struct AppliedAccessibilityUpdate {
    pub(super) changed: Vec<DocumentNodeId>,
    pub(super) full: bool,
}

impl AccessibilityDocument {
    pub(in crate::windows_app) fn apply(
        &mut self,
        document: DocumentId,
        revision: u64,
        update: AccessibilityUpdate,
    ) -> Result<AppliedAccessibilityUpdate, String> {
        if revision == 0 {
            return Err("zero accessibility revision".into());
        }
        if self.document == Some(document) && revision <= self.revision {
            return Err("stale accessibility revision".into());
        }
        let new_document = self.document != Some(document);
        if update.is_unchanged() && (new_document || self.nodes.is_empty()) {
            // The renderer builds no tree until an assistive technology is active; see
            // `BrowserMessage::AccessibilityPolicy`. A new document still retires the old tree.
            let had_tree = self.clear();
            self.document = Some(document);
            self.revision = revision;
            return Ok(AppliedAccessibilityUpdate {
                changed: Vec::new(),
                full: had_tree,
            });
        }
        if (new_document || self.nodes.is_empty()) && !update.full {
            return Err("accessibility delta arrived before its full tree".into());
        }
        if !new_document && !self.nodes.is_empty() && update.full {
            return Err("accessibility document sent a second full tree".into());
        }
        if !new_document && !self.nodes.is_empty() && update.root != self.root.unwrap() {
            return Err("accessibility document root changed".into());
        }

        let mut next = if update.full {
            HashMap::with_capacity(update.nodes.len())
        } else {
            self.nodes.clone()
        };
        let mut platform_ids = if update.full {
            HashMap::with_capacity(update.nodes.len())
        } else {
            self.platform_ids.clone()
        };
        let mut document_ids = if update.full {
            HashMap::with_capacity(update.nodes.len())
        } else {
            self.document_ids.clone()
        };
        let mut next_platform_id = if update.full {
            DOCUMENT_PLATFORM_ID_BASE
        } else {
            self.next_platform_id
        };
        for removed in &update.removed {
            if next.remove(removed).is_none() {
                return Err("accessibility delta removed an unknown node".into());
            }
            if let Some(platform_id) = platform_ids.remove(removed) {
                document_ids.remove(&platform_id);
            }
        }
        let added = update.added.iter().copied().collect::<HashSet<_>>();
        let mut changed = Vec::with_capacity(update.nodes.len());
        for node in update.nodes {
            if !update.full {
                let exists = next.contains_key(&node.id);
                if added.contains(&node.id) == exists {
                    return Err("accessibility delta has an invalid new-node marker".into());
                }
            }
            changed.push(node.id);
            if let Entry::Vacant(entry) = platform_ids.entry(node.id) {
                let platform_id = next_platform_id;
                next_platform_id = next_platform_id
                    .checked_add(1)
                    .ok_or_else(|| "AccessKit document identity space exhausted".to_string())?;
                entry.insert(platform_id);
                document_ids.insert(platform_id, node.id);
            }
            next.insert(node.id, node);
        }
        validate_tree(update.root, update.focus, &next)?;

        self.document = Some(document);
        self.revision = revision;
        self.root = Some(update.root);
        self.focus = Some(update.focus);
        self.nodes = next;
        self.platform_ids = platform_ids;
        self.document_ids = document_ids;
        self.next_platform_id = next_platform_id;
        changed.sort_by_key(|id| id.get());
        Ok(AppliedAccessibilityUpdate {
            changed,
            full: update.full || new_document,
        })
    }

    pub(in crate::windows_app) fn clear(&mut self) -> bool {
        let had_tree = !self.nodes.is_empty();
        *self = Self::default();
        had_tree
    }

    pub(in crate::windows_app) fn root(&self) -> Option<DocumentNodeId> {
        self.root
    }

    pub(in crate::windows_app) fn focus(&self) -> Option<DocumentNodeId> {
        self.focus
    }

    pub(in crate::windows_app) fn node(&self, id: DocumentNodeId) -> Option<&SemanticNode> {
        self.nodes.get(&id)
    }

    pub(in crate::windows_app) fn nodes(&self) -> impl Iterator<Item = &SemanticNode> {
        self.nodes.values()
    }

    pub(in crate::windows_app) fn platform_id(&self, id: DocumentNodeId) -> Option<u64> {
        self.platform_ids.get(&id).copied()
    }

    pub(in crate::windows_app) fn document_id_for_platform(
        &self,
        id: u64,
    ) -> Option<DocumentNodeId> {
        self.document_ids.get(&id).copied()
    }
}

fn validate_tree(
    root: DocumentNodeId,
    focus: DocumentNodeId,
    nodes: &HashMap<DocumentNodeId, SemanticNode>,
) -> Result<(), String> {
    if nodes.is_empty()
        || nodes.len() > MAX_ACCESSIBILITY_NODES
        || !nodes.contains_key(&root)
        || !nodes.contains_key(&focus)
    {
        return Err("accessibility tree has an invalid root or focus".into());
    }
    let mut parents = HashMap::with_capacity(nodes.len().saturating_sub(1));
    let mut edges = 0_usize;
    for node in nodes.values() {
        edges = edges
            .checked_add(node.children.len())
            .ok_or_else(|| "accessibility edge count overflowed".to_string())?;
        if edges > MAX_ACCESSIBILITY_EDGES {
            return Err("accessibility tree exceeded its edge budget".into());
        }
        for child in &node.children {
            if !nodes.contains_key(child) || parents.insert(*child, node.id).is_some() {
                return Err("accessibility node has an invalid or duplicate parent".into());
            }
        }
    }
    if parents.contains_key(&root) {
        return Err("accessibility root has a parent".into());
    }
    let mut visited = HashSet::with_capacity(nodes.len());
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if !visited.insert(id) {
            return Err("accessibility tree contains a cycle".into());
        }
        stack.extend(nodes[&id].children.iter().copied());
    }
    if visited.len() != nodes.len() {
        return Err("accessibility tree contains unreachable nodes".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
