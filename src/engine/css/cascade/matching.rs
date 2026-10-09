//! One applicability contract for layout, CSSOM and animation discovery.
use super::*;
use crate::engine::css::selector_match::{AncestorFilter, selector_matches_with_shadow_host_child};
use std::cell::OnceCell;

impl StyleSet {
    pub(super) fn matching_rules(
        &self,
        node: &NodeRef,
        pseudo: Option<PseudoElement>,
    ) -> Vec<&Rule> {
        if node.element().is_none() {
            return Vec::new();
        }
        let tree_root = Node::tree_root(node);
        let ancestors = OnceCell::new();
        self.compiled
            .index
            .candidates(node, pseudo)
            .into_iter()
            .filter_map(|index| self.compiled.rules.get(index))
            .filter(|rule| self.rule_matches(rule, node, pseudo, &tree_root, &ancestors))
            .collect()
    }

    pub(super) fn rule_matches(
        &self,
        rule: &Rule,
        node: &NodeRef,
        pseudo: Option<PseudoElement>,
        tree_root: &NodeRef,
        ancestors: &OnceCell<AncestorFilter>,
    ) -> bool {
        if rule.pseudo != pseudo || !rule_applies_to(rule, node, tree_root) {
            return false;
        }
        if let RuleScope::HostChild(root) = rule.scope
            && !selector_matches_with_shadow_host_child(&rule.selector, node, root)
        {
            return false;
        }
        if !rule.css_scopes.is_empty() {
            return scope::proximity(rule, node).is_some();
        }
        if matches!(rule.scope, RuleScope::HostChild(_)) {
            return true;
        }
        (!AncestorFilter::needed(&rule.selector)
            || ancestors
                .get_or_init(|| self.ancestor_filters.borrow_mut().for_node(node))
                .may_match(&rule.selector))
            && selector_matches(&rule.selector, node)
    }
}

fn rule_applies_to(rule: &Rule, node: &NodeRef, tree_root: &NodeRef) -> bool {
    if rule.part.is_some() {
        return parts::part_rule_applies(rule, node, tree_root);
    }
    let scope_matches = match rule.scope {
        RuleScope::Document => !matches!(tree_root.data, NodeData::ShadowRoot(_)),
        RuleScope::Shadow(root) => tree_root.id() == root,
        RuleScope::Host(root) => node.shadow_root().is_some_and(|shadow| shadow.id() == root),
        RuleScope::HostChild(root) => tree_root.id() == root,
        RuleScope::Slotted(root) => Node::assigned_slot(node).is_some_and(|slot| {
            Node::tree_root(&slot).id() == root
                && rule.slotted_origin.as_ref().is_some_and(|origin| {
                    if rule.slotted_host_child {
                        selector_matches_with_shadow_host_child(origin, &slot, root)
                    } else {
                        selector_matches(origin, &slot)
                    }
                })
        }),
    };
    scope_matches
        && rule.host_condition.as_ref().is_none_or(|condition| {
            let owning_host = match rule.scope {
                RuleScope::Host(_) => Some(node.clone()),
                RuleScope::Slotted(_) => {
                    Node::assigned_slot(node).and_then(|slot| Node::tree_root(&slot).shadow_host())
                }
                _ => tree_root.shadow_host(),
            };
            owning_host.is_some_and(|host| {
                if rule.host_context {
                    // CSS Shadow §3.2.3 walks the host's shadow-including ancestors.
                    // https://drafts.csswg.org/css-shadow-1/#host-context
                    std::iter::successors(Some(host), |node| node.shadow_including_parent())
                        .any(|ancestor| selector_matches(condition, &ancestor))
                } else {
                    selector_matches(condition, &host)
                }
            })
        })
}
