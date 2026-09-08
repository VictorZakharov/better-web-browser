//! The document title shares HTML's namespace, child-text and whitespace semantics.
use super::*;

pub(super) fn document_title(document: &NodeRef) -> String {
    const HTML: &str = "http://www.w3.org/1999/xhtml";
    const SVG: &str = "http://www.w3.org/2000/svg";
    let is = |node: &NodeRef, namespace: &str, name: &str| {
        node.namespace_uri() == Some(namespace) && node.tag_name() == Some(name)
    };
    let root = document
        .children
        .borrow()
        .iter()
        .find(|node| node.element().is_some())
        .cloned();
    let title = if let Some(root) = root.filter(|root| is(root, SVG, "svg")) {
        root.children
            .borrow()
            .iter()
            .find(|node| is(node, SVG, "title"))
            .cloned()
    } else {
        Node::descendants(document).find(|node| is(node, HTML, "title"))
    };
    let Some(title) = title else {
        return String::new();
    };
    let mut text = String::new();
    for child in title.children.borrow().iter() {
        if let NodeData::Text(value) = &child.data {
            text.push_str(&value.borrow());
        }
    }
    // Infra ASCII whitespace excludes vertical tab and every Unicode space (including NBSP).
    // https://html.spec.whatwg.org/multipage/dom.html#document.title
    text.split(['\t', '\n', '\u{000C}', '\r', ' '])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_document_title_uses_only_direct_namespaced_title_and_text_children() {
        let dom = Dom::default();
        let root = Node::create_element_ns_for(&dom.document, "http://www.w3.org/2000/svg", "svg");
        Node::append_child(&dom.document, root.clone());
        let nested = Node::create_element_ns_for(&dom.document, "http://www.w3.org/2000/svg", "g");
        let title =
            Node::create_element_ns_for(&dom.document, "http://www.w3.org/2000/svg", "title");
        Node::set_text_content(&title, " \t SVG\n title\u{00a0}\u{000B} ");
        Node::append_child(&nested, title.clone());
        Node::append_child(&root, nested.clone());
        assert_eq!(document_title(&dom.document), "");
        Node::append_child(&root, title.clone());
        let span = Node::create_element_for(&dom.document, "span");
        Node::set_text_content(&span, "Ignored descendant");
        Node::append_child(&title, span);
        assert_eq!(document_title(&dom.document), "SVG title\u{00a0}\u{000B}");
    }
}
