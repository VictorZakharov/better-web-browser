//! Final fail-soft enforcement for parser-owned DOM node and depth budgets.

use super::{Dom, NodeData, NodeRef};
use crate::limits::{MAX_DOM_DEPTH, MAX_DOM_NODES};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct DomLimitReport {
    pub removed_nodes: usize,
    pub depth_limited: bool,
}

pub(super) fn enforce(dom: &Dom) -> DomLimitReport {
    let mut report = DomLimitReport::default();
    let mut scheduled = 1_usize;
    let mut stack = vec![(dom.document.clone(), 0_usize)];
    while let Some((node, depth)) = stack.pop() {
        if depth >= MAX_DOM_DEPTH {
            report.depth_limited |= !node.children.borrow().is_empty();
            discard_children(&node, &mut report);
            discard_template(&node, &mut report);
            continue;
        }

        let children = std::mem::take(&mut *node.children.borrow_mut());
        let mut retained = Vec::with_capacity(children.len());
        for child in children {
            if scheduled < MAX_DOM_NODES {
                scheduled += 1;
                stack.push((child.clone(), depth + 1));
                retained.push(child);
            } else {
                child.parent.set(None);
                report.removed_nodes += discard_subtree(child);
            }
        }
        *node.children.borrow_mut() = retained;

        let template = node
            .element()
            .and_then(|element| element.template_contents.borrow_mut().take());
        if let Some(template) = template {
            if scheduled < MAX_DOM_NODES {
                scheduled += 1;
                stack.push((template.clone(), depth + 1));
                if let NodeData::Element(element) = &node.data {
                    *element.template_contents.borrow_mut() = Some(template);
                }
            } else {
                report.removed_nodes += discard_subtree(template);
            }
        }
    }
    report
}

fn discard_children(node: &NodeRef, report: &mut DomLimitReport) {
    for child in std::mem::take(&mut *node.children.borrow_mut()) {
        child.parent.set(None);
        report.removed_nodes += discard_subtree(child);
    }
}

fn discard_template(node: &NodeRef, report: &mut DomLimitReport) {
    let template = node
        .element()
        .and_then(|element| element.template_contents.borrow_mut().take());
    if let Some(template) = template {
        report.removed_nodes += discard_subtree(template);
    }
}

fn discard_subtree(root: NodeRef) -> usize {
    let mut removed = 0_usize;
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        removed += 1;
        for child in std::mem::take(&mut *node.children.borrow_mut()) {
            child.parent.set(None);
            stack.push(child);
        }
        if let Some(template) = node
            .element()
            .and_then(|element| element.template_contents.borrow_mut().take())
        {
            stack.push(template);
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dom::Node;
    use crate::engine::dom::mutation::append_node;

    #[test]
    fn discards_deep_subtrees_iteratively() {
        let dom = Dom::default();
        let mut parent = dom.document.clone();
        for _ in 0..MAX_DOM_DEPTH + 8 {
            let child = Node::create_element_for(&dom.document, "div");
            append_node(&parent, child.clone());
            parent = child;
        }

        let report = enforce(&dom);

        assert!(report.depth_limited);
        assert_eq!(Node::descendants(&dom.document).count(), MAX_DOM_DEPTH + 1);
    }
}
