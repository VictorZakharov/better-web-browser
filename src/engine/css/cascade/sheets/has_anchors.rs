//! Bounded style roots for `:has()` instead of whole-document recalculation.
//
// A relative selector is evaluated from its anchor element. A mutation below or after an
// element can only change `:has()` results for its ancestors and for earlier siblings of the
// element or its ancestors. That walk is bounded by tree depth even when a `:has()` compound
// has no keys; positive tag, ID and class keys only skip elements that cannot be anchors.
// Within the walk, an anchor needs recalculation only when one of its `:has()` results differs
// from the result its current dependents were styled with. Recalculating such an anchor's subtree
// (or its parent's when later siblings depend on it) covers every selector that consults it.
// https://drafts.csswg.org/selectors-4/#relational
use super::*;
use crate::engine::css::selector_match::relative_selector_matches;
use crate::engine::css::selector_model::{Combinator, RelativeSearch, RelativeSelector, Selector};
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod tests;

#[derive(Debug, Default)]
pub(super) struct HasAnchors {
    anchors: Vec<AnchorKey>,
    /// A `:has()` argument begins with `+` or `~`, so earlier siblings can be anchors.
    sibling_relations: bool,
    /// Some `:has()` sits inside an `of S` filter, which positions other elements.
    unbounded: bool,
}

#[derive(Debug, Default, Clone, PartialEq)]
struct AnchorKey {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    /// Later siblings (and their subtrees) can match through this anchor.
    sibling_dependents: bool,
    has: Vec<Vec<RelativeSelector>>,
}

/// `:has()` results per anchor element, in the order of its matching anchor keys.
pub(in crate::engine::css::cascade) type HasResults = HashMap<NodeId, Vec<bool>>;

pub(in crate::engine::css::cascade) struct RelationalRoots {
    pub(in crate::engine::css::cascade) roots: Vec<NodeRef>,
    /// Results to remember once these roots have been recalculated.
    pub(in crate::engine::css::cascade) results: Vec<(NodeId, Vec<bool>)>,
}

impl AnchorKey {
    fn with(&self, compound: &crate::engine::css::selector_model::CompoundSelector) -> Self {
        let mut key = self.clone();
        key.has = Vec::new();
        key.tag = compound.tag.clone().or(key.tag);
        key.id = compound.id.clone().or(key.id);
        key.classes.extend(compound.classes.iter().cloned());
        key
    }

    // ASCII case-insensitive comparison is a superset of both standards and quirks matching.
    fn may_match(&self, node: &NodeRef) -> bool {
        let Some(element) = node.element() else {
            return false;
        };
        self.tag
            .as_deref()
            .is_none_or(|tag| element.name.local.as_ref().eq_ignore_ascii_case(tag))
            && self.id.as_deref().is_none_or(|id| {
                node.attr_ref("id")
                    .is_some_and(|value| value.eq_ignore_ascii_case(id))
            })
            && (self.classes.is_empty()
                || node.attr_ref("class").is_some_and(|value| {
                    self.classes.iter().all(|wanted| {
                        value
                            .split_ascii_whitespace()
                            .any(|class| class.eq_ignore_ascii_case(wanted))
                    })
                }))
    }
}

impl HasAnchors {
    /// Whether `selector` contains a `:has()` anywhere.
    pub(super) fn is_relational(selector: &Selector) -> bool {
        let mut anchors = Self::default();
        anchors.include(selector);
        !anchors.anchors.is_empty() || anchors.unbounded
    }

    pub(super) fn include(&mut self, selector: &Selector) {
        self.visit(selector, &AnchorKey::default(), false, false);
    }

    /// `inherited` holds keys of enclosing compounds that describe the same element as this
    /// selector's rightmost compound (an `:is()`, `:where()` or `:not()` argument).
    fn visit(&mut self, selector: &Selector, inherited: &AnchorKey, later: bool, nth: bool) {
        let last = selector.compounds.len().saturating_sub(1);
        for (index, compound) in selector.compounds.iter().enumerate() {
            let sibling_dependents = later
                || selector.combinators.iter().skip(index).any(|combinator| {
                    matches!(
                        combinator,
                        Combinator::AdjacentSibling | Combinator::GeneralSibling
                    )
                });
            let base = if index == last {
                inherited.clone()
            } else {
                AnchorKey::default()
            };
            let key = AnchorKey {
                sibling_dependents,
                ..base.with(compound)
            };
            if !compound.has.is_empty() {
                self.unbounded |= nth;
                let anchor = AnchorKey {
                    has: compound.has.clone(),
                    ..key.clone()
                };
                // Selector lists often repeat one `:has()` under different ancestors
                // (`.a x:has(+ y), .b x:has(+ y)`). Results are only compared with earlier
                // results of the same anchors, so each distinct anchor is evaluated once.
                if !self.anchors.contains(&anchor) {
                    self.anchors.push(anchor);
                }
            }
            for relatives in &compound.has {
                for relative in relatives {
                    self.sibling_relations |= !matches!(
                        relative.search,
                        RelativeSearch::Descendants | RelativeSearch::Children
                    );
                    // Selectors 4 forbids nested :has(); stay conservative if one is parsed.
                    self.visit(&relative.selector, &AnchorKey::default(), false, true);
                }
            }
            for functional in &compound.functional {
                for argument in &functional.selectors {
                    self.visit(argument, &key, sibling_dependents, nth);
                }
            }
            for filter in &compound.nth {
                for argument in &filter.filter {
                    self.visit(argument, &AnchorKey::default(), sibling_dependents, true);
                }
            }
        }
    }

    /// Returns roots that cover every `:has()` result the requested roots can change, or `None`
    /// when only a whole-document refresh is conservative. Anchors whose results equal
    /// `known` keep their dependents' styles.
    pub(super) fn roots(
        &self,
        requested: &[NodeRef],
        known: &HasResults,
    ) -> Option<RelationalRoots> {
        if self.unbounded {
            return None;
        }
        let mut relational = RelationalRoots {
            roots: Vec::new(),
            results: Vec::new(),
        };
        if self.anchors.is_empty() {
            return Some(relational);
        }
        let mut visited = HashSet::new();
        let mut visit = |node: &NodeRef| {
            if visited.insert(node.id()) {
                self.add_changed_anchor(node, known, &mut relational);
            }
        };
        // Each node's walk continues to the same ancestors, so a walk ends where an earlier one
        // passed. The earlier siblings of all walked children of one parent are the children
        // before the last walked one, visited once per parent instead of once per child.
        let mut walked = HashSet::new();
        let mut parents = Vec::new();
        for root in requested {
            let mut node = root.clone();
            visit(&node);
            while walked.insert(node.id()) {
                let Some(parent) = node.parent() else {
                    break;
                };
                if self.sibling_relations && !walked.contains(&parent.id()) {
                    parents.push(parent.clone());
                }
                visit(&parent);
                node = parent;
            }
        }
        for parent in parents {
            let children = parent.children.borrow();
            let Some(last) = children
                .iter()
                .rposition(|child| walked.contains(&child.id()))
            else {
                continue;
            };
            children[..last]
                .iter()
                .filter(|child| child.element().is_some())
                .for_each(&mut visit);
        }
        Some(relational)
    }

    /// Whether a `:has()` argument searches siblings, so changing an element's siblings can
    /// change the results of anchors beside it.
    pub(super) fn sibling_relations(&self) -> bool {
        self.sibling_relations
    }

    /// Current `:has()` results of every anchor key `node` may match, and whether later
    /// siblings can depend on them; `None` when `node` cannot be an anchor.
    pub(super) fn current_results(&self, node: &NodeRef) -> Option<(Vec<bool>, bool)> {
        if self.unbounded {
            return None;
        }
        let mut siblings = false;
        let mut results = Vec::new();
        for key in self.anchors.iter().filter(|key| key.may_match(node)) {
            siblings |= key.sibling_dependents;
            results.extend(key.has.iter().map(|alternatives| {
                alternatives
                    .iter()
                    .any(|relative| relative_selector_matches(relative, node))
            }));
        }
        (!results.is_empty()).then_some((results, siblings))
    }

    fn add_changed_anchor(&self, node: &NodeRef, known: &HasResults, output: &mut RelationalRoots) {
        let Some((results, siblings)) = self.current_results(node) else {
            return;
        };
        if known.get(&node.id()) == Some(&results) {
            return;
        }
        output.roots.push(if siblings {
            node.parent().unwrap_or_else(|| node.clone())
        } else {
            node.clone()
        });
        output.results.push((node.id(), results));
    }
}
