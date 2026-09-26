//! CSS Cascade Level 5 layer identities and first-declaration ordering.
//!
//! A layer is scoped to one cascade tree (document or shadow root). Named paths may be
//! reopened across sheets; anonymous segments instead belong to one occurrence of a sheet.
//! https://drafts.csswg.org/css-cascade-5/#layer-ordering

use std::collections::HashMap;

mod syntax;

pub(crate) fn cssom_layer_names(prelude: &str, block: bool) -> Option<Vec<String>> {
    let paths = if block {
        let name = syntax::layer_prelude(prelude)?;
        if name.is_empty() {
            Vec::new()
        } else {
            vec![syntax::parse_layer_name(name)?]
        }
    } else {
        syntax::parse_layer_statement(prelude)?
    };
    paths
        .into_iter()
        .map(|path| {
            path.into_iter()
                .map(|segment| match segment {
                    LayerSegment::Named(name) => Some(name),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()
                .map(|segments| segments.join("."))
        })
        .collect()
}
pub(super) use syntax::{layer_prelude, parse_layer_name, parse_layer_statement};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum LayerSegment {
    Named(String),
    Anonymous(u64),
    ImportAnonymous(u64),
}

pub(super) type LayerPath = Vec<LayerSegment>;

#[derive(Clone, Debug)]
pub(super) enum LayerEvent {
    Declare(LayerPath),
    Rule(usize),
}

#[derive(Default)]
pub(super) struct LayerRegistry {
    nodes: Vec<LayerNode>,
    roots: Vec<usize>,
    lookup: HashMap<(Option<usize>, LayerSegment), usize>,
}

#[derive(Default)]
struct LayerNode {
    children: Vec<usize>,
}

impl LayerRegistry {
    pub(super) fn declare(&mut self, path: &[LayerSegment]) -> Option<usize> {
        let mut parent = None;
        for segment in path {
            let key = (parent, segment.clone());
            let id = if let Some(&id) = self.lookup.get(&key) {
                id
            } else {
                // CSS can declare many empty layers without producing any style rules. Keep
                // hostile stylesheets bounded independently of the qualified-rule limit.
                if self.nodes.len() >= crate::limits::MAX_CSS_RULES_PER_STYLESHEET {
                    return None;
                }
                let id = self.nodes.len();
                self.nodes.push(LayerNode::default());
                if let Some(parent) = parent {
                    self.nodes[parent].children.push(id);
                } else {
                    self.roots.push(id);
                }
                self.lookup.insert(key, id);
                id
            };
            parent = Some(id);
        }
        parent
    }

    pub(super) fn ranks(&self) -> Vec<u32> {
        let mut ranks = vec![0; self.nodes.len()];
        let mut rank = 0;
        for &root in &self.roots {
            self.rank_subtree(root, &mut rank, &mut ranks);
        }
        ranks
    }

    fn rank_subtree(&self, node: usize, next: &mut u32, ranks: &mut [u32]) {
        for &child in &self.nodes[node].children {
            self.rank_subtree(child, next, ranks);
        }
        ranks[node] = *next;
        *next = next.saturating_add(1);
    }
}

/// Replace parser-local anonymous IDs with IDs unique to one stylesheet occurrence. Named
/// components are intentionally left intact so later declarations reopen the same layer.
pub(super) fn occurrence_path(path: &[LayerSegment], occurrence: u32) -> LayerPath {
    path.iter()
        .map(|part| match part {
            LayerSegment::Named(name) => LayerSegment::Named(name.clone()),
            LayerSegment::Anonymous(local) => {
                LayerSegment::Anonymous((u64::from(occurrence) << 32) | (local & 0xffff_ffff))
            }
            LayerSegment::ImportAnonymous(id) => LayerSegment::ImportAnonymous(*id),
        })
        .collect()
}

/// One import graph is expanded for each sheet owner. Its unnamed layer IDs must be shared by
/// all descendants of that import yet not collide with another owner's identical graph.
pub(super) fn import_owner_path(path: &[LayerSegment], owner: u32) -> LayerPath {
    path.iter()
        .map(|part| match part {
            LayerSegment::ImportAnonymous(local) => {
                LayerSegment::ImportAnonymous((u64::from(owner) << 32) | (local & 0xffff_ffff))
            }
            other => other.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(name: &str) -> LayerSegment {
        LayerSegment::Named(name.into())
    }

    #[test]
    fn nested_layers_are_grouped_and_parent_rules_follow_children() {
        let mut registry = LayerRegistry::default();
        let first = registry.declare(&[name("reset")]).unwrap();
        let child = registry
            .declare(&[name("framework"), name("theme")])
            .unwrap();
        let parent = registry.declare(&[name("framework")]).unwrap();
        assert_eq!(registry.declare(&[name("reset")]), Some(first));
        let ranks = registry.ranks();
        assert!(ranks[first] < ranks[child]);
        assert!(ranks[child] < ranks[parent]);
    }

    #[test]
    fn anonymous_layers_do_not_reopen_across_sheet_occurrences() {
        let path = [LayerSegment::Anonymous(1)];
        let mut registry = LayerRegistry::default();
        let first = registry.declare(&occurrence_path(&path, 1)).unwrap();
        let second = registry.declare(&occurrence_path(&path, 2)).unwrap();
        assert_ne!(first, second);
        assert_eq!(registry.ranks(), [0, 1]);
    }
}
