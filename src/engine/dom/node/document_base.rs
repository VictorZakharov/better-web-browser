//! The document's first base element with an href.
//
// The page's and the script host's base URLs are read after every document change, each by a walk
// of the whole document, for an element most documents do not have. Either walk reads this answer.
use super::*;

#[derive(Debug)]
pub(in crate::engine::dom) struct BaseHref {
    document: NodeId,
    version: u64,
    href: Option<String>,
}

impl Node {
    /// The href of the first `base` element in tree order that has one.
    /// https://html.spec.whatwg.org/multipage/urls-and-fetching.html#document-base-url
    pub fn first_base_href(document: &NodeRef) -> Option<String> {
        let version = document.document_mutation_version();
        let mut cached = document.identity.base_href.borrow_mut();
        if let Some(base) = cached
            .as_ref()
            .filter(|base| base.document == document.id() && base.version == version)
        {
            return base.href.clone();
        }
        let href = Node::descendants(document)
            .filter(|node| node.tag_name() == Some("base"))
            .find_map(|node| node.attr("href"));
        *cached = Some(BaseHref {
            document: document.id(),
            version,
            href: href.clone(),
        });
        href
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::dom::{self, Node};

    #[test]
    fn the_answer_follows_the_document() {
        let dom = dom::parse("<!doctype html><head></head><p id=p></p>");
        assert_eq!(Node::first_base_href(&dom.document), None);
        let head = dom.elements_named("head").next().unwrap();
        let base = Node::create_element_for(&dom.document, "base");
        Node::append_child(&head, base.clone());
        assert_eq!(Node::first_base_href(&dom.document), None);
        base.set_attr("href", "https://a.test/");
        assert_eq!(
            Node::first_base_href(&dom.document).as_deref(),
            Some("https://a.test/")
        );
        let earlier = Node::create_element_for(&dom.document, "base");
        earlier.set_attr("href", "https://b.test/");
        Node::insert_before(&head, earlier.clone(), &base);
        assert_eq!(
            Node::first_base_href(&dom.document).as_deref(),
            Some("https://b.test/")
        );
        Node::remove_child(&head, &earlier);
        Node::remove_child(&head, &base);
        assert_eq!(Node::first_base_href(&dom.document), None);
    }
}
