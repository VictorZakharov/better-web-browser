//! Style sharing between elements whose matching inputs are equal.
//!
//! A full-document style pass styles many elements, such as table cells, list items and
//! links, whose matched rules and cascade inputs equal a recently styled sibling's or cousin's.
//! Like Blink's and Gecko's style sharing caches, this module reuses that element's computed
//! style instead of matching and cascading again. A candidate must:
//!
//! - have the same parent, or a parent that itself shared its style with the candidate's
//!   parent ("share group"), so inherited values and every ancestor's matching inputs agree;
//! - have the same local name and attribute list, compared by value except for attributes that
//!   only selectors read (`href`, `title`, `data-*`, ...), and the same rule-index candidates;
//! - produce the same result for every candidate rule whose selector reads inputs that
//!   comparison leaves open (Gecko's "revalidation selectors": selector-only attribute values,
//!   structural and state pseudo-classes, `:has()`, sibling combinators, ...). Rules that read
//!   only names, classes, ids, compared attributes and `:lang()` through descendant and child
//!   combinators need no check.
//!
//! Elements whose style depends on inputs this comparison does not capture are never shared;
//! see [`StyleSet::sharing_eligible`]. Debug builds recompute every hit and assert equality.
use super::*;
use crate::engine::css::selector_match::{AncestorFilter, visited};
use rules::{RuleSharing, classify_rules, referenced_ids};
use std::cell::OnceCell;

mod rules;

/// Candidates kept per parent share group, most recent first. Gecko keeps one small LRU for
/// the whole traversal, where a deep earlier sibling's subtree evicts its siblings; keying by
/// group keeps siblings and cousins available however large the subtrees between them are.
const MAX_CANDIDATES_PER_GROUP: usize = 8;

/// Identifies elements whose matching inputs and inherited values are interchangeable.
pub(super) type ShareGroup = u32;

struct Candidate {
    node: NodeRef,
    group: ShareGroup,
    revalidation: Option<Vec<bool>>,
}

/// Per-rule-set tables, built on first use and kept with the compiled rules: a load runs many
/// small restyles, and building these for each traversal's cache cost more than sharing saved.
#[derive(Debug, Default)]
pub(super) struct SharingTables {
    rules: OnceCell<Vec<RuleSharing>>,
    /// Ids any selector names, ASCII-lowercased so quirks-mode matching is covered too.
    referenced_ids: OnceCell<HashSet<String>>,
}

#[derive(Default)]
pub(super) struct SharingCache {
    candidates: HashMap<ShareGroup, std::collections::VecDeque<Candidate>>,
    /// Text styles by parent style allocation. The parent is kept alive so its address
    /// cannot be reused by another style during the traversal.
    text: HashMap<*const ComputedStyle, (Arc<ComputedStyle>, Arc<ComputedStyle>)>,
    next_group: ShareGroup,
    #[cfg(test)]
    pub(super) stats: SharingStats,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SharingStats {
    pub(super) lookups: usize,
    pub(super) hits: usize,
    pub(super) no_candidate: usize,
    pub(super) revalidation_differs: usize,
}

#[cfg(test)]
thread_local! {
    pub(super) static LAST_STATS: std::cell::Cell<SharingStats> = Default::default();
}

impl SharingCache {
    pub(super) fn new_group(&mut self) -> ShareGroup {
        self.next_group += 1;
        self.next_group
    }

    fn insert(&mut self, node: &NodeRef, parent_group: ShareGroup, group: ShareGroup) {
        let candidates = self.candidates.entry(parent_group).or_default();
        if candidates.len() == MAX_CANDIDATES_PER_GROUP {
            candidates.pop_back();
        }
        candidates.push_front(Candidate {
            node: node.clone(),
            group,
            revalidation: None,
        });
    }
}

impl StyleSet {
    /// Styles `node`, reusing a candidate's style when their inputs are equal, and returns
    /// the share group its children use as their parent group.
    pub(super) fn compute_style_shared(
        &self,
        cache: &mut SharingCache,
        node: &NodeRef,
        parent: Option<&ComputedStyle>,
        parent_group: ShareGroup,
    ) -> (Arc<ComputedStyle>, ShareGroup) {
        let eligible = self.sharing_eligible(node);
        if eligible && let Some((style, group)) = self.shared_style(cache, node, parent_group) {
            #[cfg(debug_assertions)]
            assert_eq!(
                *style,
                self.compute_style(node, parent),
                "shared style differs from a full cascade for {:?}",
                node.tag_name()
            );
            return (style, group);
        }
        let style = Arc::new(self.compute_style(node, parent));
        let group = cache.new_group();
        if eligible {
            cache.insert(node, parent_group, group);
        }
        (style, group)
    }

    /// A text node's style when another text node with the same parent style was already
    /// styled in this traversal. Text has no attributes or matching rules of its own: its style
    /// inherits from its parent, except for a closed `details` element's hidden content and for
    /// slotted text, which never share.
    pub(super) fn shared_text_style(
        &self,
        cache: &mut SharingCache,
        node: &NodeRef,
        parent: &Arc<ComputedStyle>,
    ) -> Arc<ComputedStyle> {
        let shareable = matches!(node.data, NodeData::Text(_))
            && node.parent().is_some_and(|parent| {
                parent.element().is_some() && parent.tag_name() != Some("details")
            })
            && Node::assigned_slot(node).is_none();
        if !shareable {
            return Arc::new(self.compute_style(node, Some(parent)));
        }
        if let Some((_, style)) = cache.text.get(&Arc::as_ptr(parent)) {
            #[cfg(debug_assertions)]
            assert_eq!(**style, self.compute_style(node, Some(parent)));
            return Arc::clone(style);
        }
        let style = Arc::new(self.compute_style(node, Some(parent)));
        cache.text.insert(
            Arc::as_ptr(parent),
            (Arc::clone(parent), Arc::clone(&style)),
        );
        style
    }

    fn shared_style(
        &self,
        cache: &mut SharingCache,
        node: &NodeRef,
        parent_group: ShareGroup,
    ) -> Option<(Arc<ComputedStyle>, ShareGroup)> {
        #[cfg(test)]
        {
            cache.stats.lookups += 1;
        }
        let candidate = cache
            .candidates
            .get_mut(&parent_group)
            .and_then(|candidates| {
                candidates
                    .iter_mut()
                    .find(|candidate| same_local_inputs(&candidate.node, node))
            });
        #[cfg(test)]
        {
            cache.stats.no_candidate += usize::from(candidate.is_none());
        }
        let candidate = candidate?;
        let rules = self
            .compiled
            .sharing
            .rules
            .get_or_init(|| classify_rules(&self.compiled.rules));
        let revalidation = self.revalidation_results(node, rules)?;

        if candidate.revalidation.is_none() {
            candidate.revalidation = Some(self.revalidation_results(&candidate.node, rules)?);
        }
        if candidate.revalidation.as_ref() != Some(&revalidation) {
            #[cfg(test)]
            {
                cache.stats.revalidation_differs += 1;
            }
            return None;
        }
        let style = Arc::clone(self.styles.get(&node_id(&candidate.node))?);
        let group = candidate.group;
        #[cfg(test)]
        {
            cache.stats.hits += 1;
        }
        Some((style, group))
    }

    /// Whether `node`'s style depends only on inputs the cache compares.
    fn sharing_eligible(&self, node: &NodeRef) -> bool {
        let Some(element) = node.element() else {
            return false;
        };
        let tag = element.name.local.as_ref();
        if element.name.ns.as_ref() != "http://www.w3.org/1999/xhtml"
            // Custom elements match :defined and custom states.
            || tag.contains('-')
            || ineligible_tag(tag)
            || element.shadow_root.borrow().is_some()
            || element.animation_style.borrow().is_some()
            || element.transition_style.borrow().is_some()
            || element.fullscreen.get()
            || element.popover_order.get() != 0
        {
            return false;
        }
        {
            let attributes = element.attrs.borrow();
            if attributes.iter().any(|attribute| {
                let name = attribute.name.local.as_ref();
                // Inline declarations and popover/dir=auto UA rules are per element. An id
                // matters only when some selector names it.
                matches!(name, "style" | "popover")
                    || (name == "id"
                        && self
                            .compiled
                            .sharing
                            .referenced_ids
                            .get_or_init(|| referenced_ids(&self.compiled.rules))
                            .contains(&attribute.value.to_ascii_lowercase()))
                    || (name == "dir" && attribute.value.trim().eq_ignore_ascii_case("auto"))
            }) {
                return false;
            }
        }
        let Some(parent) = node.parent() else {
            return false;
        };
        // The root inherits nothing; children of `details` depend on their summary position.
        if !matches!(parent.data, NodeData::Element(_)) || parent.tag_name() == Some("details") {
            return false;
        }
        !matches!(Node::tree_root(node).data, NodeData::ShadowRoot(_))
            && Node::assigned_slot(node).is_none()
            && !(self.visited_link_colors
                && visited::is_visited_link(node, &self.document_base_url))
    }

    /// Results of `node`'s candidate rules that need revalidation, or `None` when a candidate
    /// rule makes the element ineligible.
    fn revalidation_results(&self, node: &NodeRef, rules: &[RuleSharing]) -> Option<Vec<bool>> {
        let tree_root = Node::tree_root(node);
        let ancestors = OnceCell::<AncestorFilter>::new();
        let mut results = Vec::new();
        for index in self.compiled.index.candidates(node, None) {
            match rules.get(index).copied()? {
                RuleSharing::KeyDetermined => {}
                RuleSharing::Unshareable => return None,
                RuleSharing::Revalidate => {
                    let rule = self.compiled.rules.get(index)?;
                    results.push(self.rule_matches(rule, node, None, &tree_root, &ancestors));
                }
            }
        }
        Some(results)
    }
}

/// Form controls carry state outside attributes; the others have per-element UA rendering.
fn ineligible_tag(tag: &str) -> bool {
    matches!(
        tag,
        "input"
            | "select"
            | "textarea"
            | "button"
            | "option"
            | "optgroup"
            | "fieldset"
            | "legend"
            | "label"
            | "output"
            | "progress"
            | "meter"
            | "details"
            | "summary"
            | "dialog"
            | "slot"
            | "template"
            | "bdi"
            | "html"
            | "body"
    )
}

/// Attributes no style code reads except through selectors, which revalidation compares.
fn selector_only_attribute(name: &str) -> bool {
    matches!(
        name,
        // Eligible elements' ids are named by no selector.
        "id" | "href"
            | "title"
            | "src"
            | "srcset"
            | "sizes"
            | "alt"
            | "target"
            | "loading"
            | "decoding"
    ) || name.starts_with("data-")
        || name.starts_with("aria-")
}

fn same_local_inputs(candidate: &NodeRef, node: &NodeRef) -> bool {
    let (Some(candidate), Some(element)) = (candidate.element(), node.element()) else {
        return false;
    };
    if candidate.name != element.name {
        return false;
    }
    let (left, right) = (candidate.attrs.borrow(), element.attrs.borrow());
    left.len() == right.len()
        && left.iter().zip(right.iter()).all(|(left, right)| {
            left.name == right.name
                && (left.value == right.value
                    || (left.name.ns.is_empty() && selector_only_attribute(&left.name.local)))
        })
}
