//! The elements that name resources, collected once per document state.
//
// Resource discovery runs after every parser step and every resource completion. Network hints,
// document-load blockers, per-element resource events, link and image discovery and style sheet
// discovery each walked the whole shadow-including document for its few links, images and style
// elements, and three of them resolved every image URL again: on a long article, five walks of
// ~24k nodes per parser step, most of them after a completion that changed nothing in the
// document. The elements are kept in tree order, which resource events fire in and discovery
// reads them in, with their resource event keys, until the document changes or what their URLs
// resolve against does.
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

    /// `resources::discover_link_and_image_resources` for this page's document, read from the
    /// collected elements. An image element's event key is its image resource, resolved as
    /// discovery resolves it, so discovery reuses it.
    pub(super) fn discover_link_and_image_resources(&self) -> Vec<PageResource> {
        let elements = self.resource_elements();
        let mut discovery = super::resources::LinkAndImageDiscovery::new(
            &self.source_url,
            &self.base_url,
            self.media_environment,
        );
        for element in elements.iter() {
            if let Some(node) = element.node() {
                discovery.include(&node, || match &element.key {
                    Some(PageResource::Image { url }) => Some(url.clone()),
                    _ => None,
                });
            }
        }
        discovery.finish()
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

    #[test]
    fn discovery_from_the_elements_matches_a_document_walk() {
        let page = Page::parse(
            "<link rel=preconnect href='https://cdn.example.test/'>             <link rel='stylesheet preload' as=style href=a.css>             <link rel=prefetch href=next.html>             <img src=one.png><img srcset='two.png 2x, one.png 1x'>             <picture><source srcset=wide.png><img src=narrow.png></picture>             <input type=image src=button.png><input src=ignored.png>             <div><template shadowrootmode=open><img src=shadow.png>             <link rel=stylesheet href=shadow.css></template></div>             <p><img data-src=lazy.png src=placeholder.png></p>",
            "https://example.test/dir/page",
        );
        let walk = |page: &Page| {
            super::super::resources::discover_link_and_image_resources(
                &page.dom,
                &page.source_url,
                &page.base_url,
                page.media_environment,
            )
        };
        let kinds = |resources: &[PageResource]| {
            resources
                .iter()
                .map(|resource| match resource {
                    PageResource::OriginHint { .. } => "hint",
                    PageResource::Prefetch { .. } => "prefetch",
                    PageResource::Preload { .. } => "preload",
                    PageResource::Stylesheet { .. } => "stylesheet",
                    PageResource::Image { .. } => "image",
                    _ => "other",
                })
                .collect::<Vec<_>>()
        };
        let found = page.discover_link_and_image_resources();
        assert_eq!(found, walk(&page));
        let found_kinds = kinds(&found);
        for kind in ["hint", "prefetch", "preload", "stylesheet", "image"] {
            assert!(found_kinds.contains(&kind), "{kind} in {found:?}");
        }
        assert!(found.contains(&PageResource::Image {
            url: "https://example.test/dir/shadow.png".into()
        }));
        // Images follow every link-derived resource.
        let first_image = found_kinds
            .iter()
            .position(|kind| *kind == "image")
            .unwrap();
        assert!(
            found_kinds[first_image..]
                .iter()
                .all(|kind| *kind == "image")
        );

        let img = page.dom.elements_named("img").next().unwrap();
        img.set_attr("src", "three.png");
        let paragraph = page.dom.elements_named("p").next().unwrap();
        let link = Node::create_element_for(&page.dom.document, "link");
        link.set_attr("rel", "stylesheet");
        link.set_attr("href", "late.css");
        Node::append_child(&paragraph, link);
        let changed = page.discover_link_and_image_resources();
        assert_eq!(changed, walk(&page));
        for resource in [
            PageResource::Image {
                url: "https://example.test/dir/three.png".into(),
            },
            PageResource::Stylesheet {
                url: "https://example.test/dir/late.css".into(),
            },
        ] {
            assert!(changed.contains(&resource), "{resource:?} in {changed:?}");
        }
    }
}
