//! Same-tree SVG dependency closure; never an external-resource loader.
use crate::engine::dom::{Node, NodeData, NodeId, NodeRef};
use crate::limits::{MAX_DOM_NODES, MAX_SVG_SOURCE_BYTES};
use cssparser::{Parser, ParserInput};
use std::collections::{HashSet, VecDeque};
use std::rc::Rc;
mod index;
pub(super) use index::IndexCache;

pub(super) struct References {
    pub targets: Vec<NodeRef>,
    index: Option<Rc<index::Index>>,
    pub blocked_uses: HashSet<NodeId>,
}

impl References {
    pub fn collect_with_index(root: &NodeRef, cache: &mut IndexCache) -> Result<Self, String> {
        let mut queue = VecDeque::new();
        let mut included = HashSet::new();
        scan(root, &mut included, &mut queue)?;
        let mut result = Self {
            targets: Vec::new(),
            index: None,
            blocked_uses: HashSet::new(),
        };
        if queue.is_empty() {
            return Ok(result);
        }
        let index = cache.get(root)?;
        result.blocked_uses = super::expansion::blocked_uses(root, &index.ids)?;
        let mut seen = HashSet::new();
        while let Some(id) = queue.pop_front() {
            if !seen.insert(id.clone()) {
                continue;
            }
            if seen.len() > 256 {
                return Err("SVG exceeds reference budget".into());
            }
            let Some(target) = index.ids.get(&id) else {
                continue;
            };
            if target.namespace_uri() != Some("http://www.w3.org/2000/svg")
                || included.contains(&target.id())
            {
                continue;
            }
            scan(target, &mut included, &mut queue)?;
            result.targets.push(target.clone());
        }
        // A later dependency can contain an earlier one. Emit the containing
        // subtree once, retaining the child's position inside that subtree.
        let roots = result
            .targets
            .iter()
            .map(|node| node.id())
            .collect::<HashSet<_>>();
        result.targets.retain(|node| {
            !std::iter::successors(node.parent(), |parent| parent.parent())
                .any(|parent| roots.contains(&parent.id()))
        });
        result.index = Some(index);
        Ok(result)
    }

    pub fn retain_id(&self, id: &str, node: NodeId) -> bool {
        self.index
            .as_ref()
            .is_none_or(|index| index.retain_id(id, node))
    }
}

fn scan(
    root: &NodeRef,
    included: &mut HashSet<NodeId>,
    queue: &mut VecDeque<String>,
) -> Result<(), String> {
    for node in Node::descendants(root) {
        if !included.insert(node.id()) {
            continue;
        }
        if included.len() > MAX_DOM_NODES {
            return Err("SVG exceeds node budget".into());
        }
        // Only rendering references. An anchor is navigation, not a drawing dependency.
        if matches!(
            node.tag_name(),
            Some("use" | "linearGradient" | "radialGradient" | "pattern" | "textPath")
        ) && let Some(href) = node
            .attr_ns(None, "href")
            .or_else(|| node.attr_ns(Some("http://www.w3.org/1999/xlink"), "href"))
        {
            enqueue(&href, queue)?;
        }
        let NodeData::Element(element) = &node.data else {
            continue;
        };
        for attribute in element.attrs.borrow().iter() {
            if !super::urls::is_paint_attribute(attribute.name.local.as_ref()) {
                continue;
            }
            if attribute.value.len() > MAX_SVG_SOURCE_BYTES {
                return Err("SVG attribute exceeds source budget".into());
            }
            let mut input = ParserInput::new(&attribute.value);
            let mut parser = Parser::new(&mut input);
            while !parser.is_exhausted() {
                if let Ok(url) = parser.try_parse(|parser| parser.expect_url()) {
                    enqueue(&url, queue)?;
                } else {
                    let _ = parser.next();
                }
            }
        }
    }
    Ok(())
}

fn enqueue(url: &str, queue: &mut VecDeque<String>) -> Result<(), String> {
    if url.len() > MAX_SVG_SOURCE_BYTES {
        return Err("SVG URL exceeds source budget".into());
    }
    if let Some(id) = super::urls::fragment(url) {
        if queue.iter().any(|queued| queued == &id) {
            return Ok(());
        }
        let queued_bytes = queue.iter().map(String::len).sum::<usize>();
        if id.len() > MAX_SVG_SOURCE_BYTES.saturating_sub(queued_bytes) || queue.len() >= 256 {
            return Err("SVG reference queue exceeds budget".into());
        }
        queue.push_back(id);
    }
    Ok(())
}
