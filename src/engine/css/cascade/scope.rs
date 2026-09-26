//! Runtime matching for CSS Cascade 6 @scope chains.
use super::*;
use crate::engine::css::selector_match::selector_matches_with_scope;
use crate::engine::css::stylesheet::CssScope;

/// Return the smallest hop count to an innermost scoping root that matches this rule.
pub(super) fn proximity(rule: &Rule, subject: &NodeRef) -> Option<usize> {
    if rule.css_scopes.is_empty() {
        return selector_matches(&rule.selector, subject).then_some(usize::MAX);
    }
    let mut ancestors = std::iter::successors(Some(subject.clone()), |node| node.parent())
        .filter(|node| node.element().is_some() || matches!(node.data, NodeData::Document))
        .collect::<Vec<_>>();
    ancestors.reverse();
    let subject = ancestors.last()?;
    // A chain can have many overlapping roots. Cache the remaining match for each
    // (scope depth, outer root) pair instead of exploring every root combination.
    let context = ScopeMatchContext {
        scopes: &rule.css_scopes,
        ancestors: &ancestors,
        subject,
        selector: &rule.selector,
    };
    let mut cache = ScopeMatchCache {
        memo: std::collections::HashMap::new(),
        limits: vec![vec![None; ancestors.len()]; rule.css_scopes.len()],
    };
    matching_scope(&context, 0, None, &mut cache)
}

struct ScopeMatchContext<'a> {
    scopes: &'a [CssScope],
    ancestors: &'a [NodeRef],
    subject: &'a NodeRef,
    selector: &'a Selector,
}

struct ScopeMatchCache {
    memo: std::collections::HashMap<(usize, Option<usize>), Option<usize>>,
    limits: Vec<Vec<Option<bool>>>,
}

fn matching_scope(
    context: &ScopeMatchContext<'_>,
    depth: usize,
    outer_root: Option<usize>,
    cache: &mut ScopeMatchCache,
) -> Option<usize> {
    if let Some(cached) = cache.memo.get(&(depth, outer_root)) {
        return *cached;
    }
    let scope = &context.scopes[depth];
    let mut closest = None;
    for (root_index, root) in context
        .ancestors
        .iter()
        .enumerate()
        .skip(outer_root.unwrap_or(0))
    {
        if !is_root(
            scope,
            root,
            outer_root.map(|index| context.ancestors[index].id()),
        ) {
            continue;
        }
        let blocked = if let Some(cached) = cache.limits[depth][root_index] {
            cached
        } else {
            let blocked = has_limit(scope, &context.ancestors[root_index + 1..], root.id());
            cache.limits[depth][root_index] = Some(blocked);
            blocked
        };
        if blocked {
            continue;
        }
        let hops = if depth + 1 == context.scopes.len() {
            selector_matches_with_scope(context.selector, context.subject, Some(root.id()))
                .then_some(context.ancestors.len() - 1 - root_index)
        } else {
            matching_scope(context, depth + 1, Some(root_index), cache)
        };
        if let Some(hops) = hops {
            closest = Some(closest.map_or(hops, |earlier: usize| earlier.min(hops)));
            if hops == 0 {
                break;
            }
        }
    }
    cache.memo.insert((depth, outer_root), closest);
    closest
}

fn is_root(scope: &CssScope, candidate: &NodeRef, outer_root: Option<NodeId>) -> bool {
    scope.start.as_ref().map_or_else(
        || scope.implicit_root == Some(candidate.id()),
        |selectors| {
            selectors
                .iter()
                .any(|selector| selector_matches_with_scope(selector, candidate, outer_root))
        },
    )
}

fn has_limit(scope: &CssScope, path: &[NodeRef], root: NodeId) -> bool {
    scope.end.as_ref().is_some_and(|selectors| {
        path.iter().any(|candidate| {
            selectors
                .iter()
                .any(|selector| selector_matches_with_scope(selector, candidate, Some(root)))
        })
    })
}

#[cfg(test)]
mod tests;
