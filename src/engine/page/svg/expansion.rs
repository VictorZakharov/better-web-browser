//! Bound actual use-instance expansion, not merely the number of unique IDs.
use crate::engine::dom::{Node, NodeId, NodeRef};
use crate::limits::MAX_DOM_NODES;
use std::collections::{HashMap, HashSet};

pub(super) fn blocked_uses(
    root: &NodeRef,
    index: &HashMap<String, NodeRef>,
) -> Result<HashSet<NodeId>, String> {
    let mut blocked = HashSet::new();
    let mut stack = vec![(root.clone(), vec![root.id()])];
    let mut pending_path_nodes = 1;
    let mut visited = 0;
    while let Some((subtree, path)) = stack.pop() {
        pending_path_nodes -= path.len();
        for node in Node::descendants(&subtree) {
            visited += 1;
            if visited > MAX_DOM_NODES {
                return Err("SVG use expansion exceeds node budget".into());
            }
            if node.tag_name() != Some("use") {
                continue;
            }
            let href = node
                .attr_ns(None, "href")
                .or_else(|| node.attr_ns(Some("http://www.w3.org/1999/xlink"), "href"));
            let Some(target) = href
                .as_deref()
                .and_then(super::urls::fragment)
                .and_then(|id| index.get(&id))
            else {
                continue;
            };
            if target.namespace_uri() != Some("http://www.w3.org/2000/svg") {
                continue;
            }
            if path.contains(&target.id())
                || std::iter::successors(Some(node.clone()), |node| node.parent())
                    .any(|parent| parent.id() == target.id())
            {
                blocked.insert(node.id());
                continue;
            }
            if path.len() >= 256 {
                return Err("SVG use expansion exceeds depth budget".into());
            }
            pending_path_nodes += path.len() + 1;
            if pending_path_nodes > MAX_DOM_NODES {
                return Err("SVG use expansion exceeds pending-work budget".into());
            }
            let mut next = path.clone();
            next.push(target.id());
            stack.push((target.clone(), next));
        }
    }
    Ok(blocked)
}
