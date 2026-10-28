//! The elements that name resources, collected once per document state.
//
// Resource discovery runs after every parser step and every resource completion. Network hints,
// document-load blockers and per-element resource events each walked the whole shadow-including
// document for its few links, images and style elements, and the latter two resolved every image
// URL again: on a long article, three walks of ~24k nodes per step, most of them after a
// completion that changed nothing in the document. The elements are kept in tree order, which
// resource events fire in, with their resource event keys, until the document changes or what
// their URLs resolve against does.
use super::*;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

#[derive(Debug)]
pub(crate) struct ResourceElement {
    node: Weak<Node>,
    /// `Page::resource_event_key` of the element.
    pub(crate) key: Option<PageResource>,
}

impl ResourceElement {
    pub(crate) fn node(&self) -> Option<NodeRef> {
        self.node.upgrade()
    }
}

#[derive(Debug, Default)]
pub(super) struct ResourceElements(RefCell<Option<Collected>>);

#[derive(Debug)]
struct Collected {
    state: State,
    elements: Rc<[ResourceElement]>,
}

/// Everything a resource event key depends on.
#[derive(Debug, PartialEq)]
struct State {
    document: NodeId,
    version: u64,
    source_url: String,
    base_url: String,
    environment: MediaEnvironment,
}

/// Elements whose resources discovery reads: links, images, image inputs and style elements.
fn names_resources(node: &NodeRef) -> bool {
    matches!(
        node.tag_name(),
        Some("link" | "img" | "image" | "input" | "style")
    )
}

impl Page {
    /// The document's links, images, inputs and style elements in shadow-including tree order.
    pub(crate) fn resource_elements(&self) -> Rc<[ResourceElement]> {
        let state = State {
            document: self.dom.document.id(),
            version: self.dom.document.document_mutation_version(),
            source_url: self.source_url.clone(),
            base_url: self.base_url.clone(),
            environment: self.media_environment,
        };
        let mut collected = self.resource_elements.0.borrow_mut();
        if let Some(collected) = collected.as_ref().filter(|found| found.state == state) {
            return Rc::clone(&collected.elements);
        }
        let elements = Node::shadow_including_descendants(&self.dom.document)
            .filter(names_resources)
            .map(|node| ResourceElement {
                key: self.resource_event_key(&node),
                node: Rc::downgrade(&node),
            })
            .collect::<Rc<[_]>>();
        *collected = Some(Collected {
            state,
            elements: Rc::clone(&elements),
        });
        elements
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elements_follow_document_changes_in_tree_order() {
        let page = Page::parse(
            "<link rel=stylesheet href=a.css><p>text</p><img src=one.png><style>p{}</style>",
            "https://example.test/dir/page",
        );
        let names = |page: &Page| {
            page.resource_elements()
                .iter()
                .map(|element| {
                    let node = element.node().unwrap();
                    (node.tag_name().unwrap().to_string(), element.key.clone())
                })
                .collect::<Vec<_>>()
        };
        let image = |url: &str| Some(PageResource::Image { url: url.into() });
        let first = names(&page);
        assert_eq!(first.len(), 3);
        assert_eq!(
            first[1],
            ("img".into(), image("https://example.test/dir/one.png"))
        );
        assert!(Rc::ptr_eq(
            &page.resource_elements(),
            &page.resource_elements()
        ));

        let img = page.dom.elements_named("img").next().unwrap();
        img.set_attr("src", "two.png");
        let paragraph = page.dom.elements_named("p").next().unwrap();
        let input = Node::create_element_for(&page.dom.document, "input");
        input.set_attr("type", "image");
        input.set_attr("src", "three.png");
        Node::append_child(&paragraph, input);
        let changed = names(&page);
        assert_eq!(
            changed
                .iter()
                .map(|(name, key)| (name.as_str(), key.clone()))
                .collect::<Vec<_>>(),
            [
                ("link", first[0].1.clone()),
                ("input", image("https://example.test/dir/three.png")),
                ("img", image("https://example.test/dir/two.png")),
                ("style", None),
            ]
        );
    }
}
