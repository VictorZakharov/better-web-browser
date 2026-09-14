//! HTML fragment selection, shared by scripted and scriptless hyperlink activation.
//! https://html.spec.whatwg.org/multipage/browsing-the-web.html#scroll-to-the-fragment
use super::dom::{Node, NodeId, NodeRef};
use super::layout::{RectF, ScrollBox};
use std::collections::HashMap;

pub(crate) fn is_same_document(current: &str, target: &str) -> bool {
    let (Ok(mut current), Ok(mut target)) = (url::Url::parse(current), url::Url::parse(target))
    else {
        return false;
    };
    if target.fragment().is_none() {
        return false;
    }
    current.set_fragment(None);
    target.set_fragment(None);
    current == target
}

pub(crate) enum FragmentTarget {
    Top,
    Element(NodeRef),
}

pub(crate) fn select(document: &NodeRef, url: &str) -> Option<FragmentTarget> {
    let url = url::Url::parse(url).ok()?;
    let fragment = url.fragment()?;
    if fragment.is_empty() {
        return Some(FragmentTarget::Top);
    }
    let find = |value: &str| {
        Node::descendants(document)
            .find(|node| node.attr("id").as_deref() == Some(value))
            .or_else(|| {
                Node::descendants(document).find(|node| {
                    node.tag_name() == Some("a") && node.attr("name").as_deref() == Some(value)
                })
            })
    };
    if let Some(node) = find(fragment) {
        return Some(FragmentTarget::Element(node));
    }
    // Percent-decode bytes without form decoding: '+' is a literal fragment character.
    let mut bytes = Vec::with_capacity(fragment.len());
    let mut input = fragment.as_bytes().iter().copied().peekable();
    while let Some(byte) = input.next() {
        let mut lookahead = input.clone();
        let hex = lookahead
            .next()
            .and_then(|v| (v as char).to_digit(16))
            .zip(lookahead.next().and_then(|v| (v as char).to_digit(16)));
        if byte == b'%'
            && let Some((high, low)) = hex
        {
            bytes.push((high * 16 + low) as u8);
            input = lookahead;
        } else {
            bytes.push(byte);
        }
    }
    let decoded = String::from_utf8_lossy(&bytes);
    find(&decoded).map(FragmentTarget::Element).or_else(|| {
        decoded
            .eq_ignore_ascii_case("top")
            .then_some(FragmentTarget::Top)
    })
}

pub(crate) fn scroll_to_fragment(
    document: &NodeRef,
    url: &str,
    geometry: &HashMap<NodeId, RectF>,
    scroll_boxes: &HashMap<NodeId, ScrollBox>,
    max_scroll: f32,
) -> Option<f32> {
    let target = match select(document, url)? {
        FragmentTarget::Top => return Some(0.0),
        FragmentTarget::Element(node) => node,
    };
    let mut rect = *geometry.get(&target.id())?;
    // Align each scrolling ancestor before the viewport, using the unscrolled layout boxes.
    for parent in std::iter::successors(Node::composed_parent(&target), Node::composed_parent) {
        if let Some(scroll) = scroll_boxes.get(&parent.id()) {
            let (old_x, old_y) = parent.scroll_offset.get();
            let x = if rect.x < scroll.port.x + old_x {
                rect.x - scroll.port.x
            } else if rect.right() > scroll.port.right() + old_x {
                rect.right() - scroll.port.right()
            } else {
                old_x
            };
            let (x, y) = scroll.clamp(x, rect.y - scroll.port.y);
            parent.scroll_offset.set((x, y));
            rect.x -= if scroll.scroll_x { x } else { old_x };
            rect.y -= if scroll.scroll_y { y } else { old_y };
        }
    }
    Some(rect.y.clamp(0.0, max_scroll.max(0.0)))
}

#[cfg(test)]
mod tests;
