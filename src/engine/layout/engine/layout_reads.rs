//! Records the page inputs outside the DOM and computed styles that a cached layout result
//! read, so a later pass can tell whether the result still holds.
//
// For an unchanged DOM and style, these inputs are: which URL an element shows
// (`Page::image_url`), the natural dimensions of a decoded image or inline SVG, a scroll
// container's offset, and two values read from ancestors outside a subtree: the link a promoted
// inline formatting root inherits and the backdrop a form control composites. Every intrinsic computation that a cache may return later is
// computed inside a frame; reads made while a frame is open land in it, and a cache hit
// replays the hit's recorded reads into the enclosing frame. Closing a frame also passes its
// reads to the enclosing frame, so every cached result lists every read it depended on,
// however it was assembled.
use super::*;
use std::collections::HashSet;
use std::rc::Rc;

/// A node a read was made for. Identity is the node ID; the weak reference finds the node
/// again without searching the document or keeping a removed node alive.
#[derive(Clone, Debug)]
pub(in crate::engine::layout) struct ReadNode(NodeId, std::rc::Weak<Node>);

impl ReadNode {
    fn new(node: &NodeRef) -> Self {
        Self(node.id(), Rc::downgrade(node))
    }

    /// The node, while it is still connected to `page`'s document.
    fn get(&self, page: &Page) -> Option<NodeRef> {
        self.1
            .upgrade()
            .filter(|node| Node::shadow_including_root(node).id() == page.dom.document.id())
    }
}

impl PartialEq for ReadNode {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for ReadNode {}

impl std::hash::Hash for ReadNode {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(in crate::engine::layout) enum LayoutRead {
    ImageUrl(ReadNode, Option<String>),
    DecodedSize(String, Option<(u32, u32)>),
    LayoutDimensions(String, Option<(u32, u32)>),
    ScrollOffset(ReadNode, (u32, u32)),
    InheritedLink(ReadNode, Option<(String, NodeId)>),
    Backdrop(ReadNode, (u8, u8, u8, u8)),
}

impl LayoutRead {
    /// Whether `page` and `styles` would still answer this read the same way.
    fn holds(&self, page: &Page, styles: &StyleSet) -> bool {
        match self {
            Self::ScrollOffset(node, offset) => node
                .get(page)
                .is_some_and(|node| scroll_offset_bits(&node) == *offset),
            Self::InheritedLink(node, link) => node
                .get(page)
                .is_some_and(|node| inherited_link(page, &node) == *link),
            Self::Backdrop(node, color) => node
                .get(page)
                .is_some_and(|node| color_bits(effective_backdrop(styles, &node)) == *color),
            Self::ImageUrl(node, url) => node
                .get(page)
                .is_some_and(|node| page.image_url(&node) == *url),
            Self::DecodedSize(url, size) => decoded_size(page, url) == *size,
            Self::LayoutDimensions(key, dimensions) => {
                page.layout_image_dimensions(key) == *dimensions
            }
        }
    }
}

/// The reads one cached result depended on; usually empty.
#[derive(Clone, Debug, Default)]
pub(in crate::engine::layout) struct LayoutReads(Option<Rc<[LayoutRead]>>);

impl LayoutReads {
    pub(in crate::engine::layout) fn hold(&self, page: &Page, styles: &StyleSet) -> bool {
        self.0
            .as_deref()
            .is_none_or(|reads| reads.iter().all(|read| read.holds(page, styles)))
    }
}

#[derive(Default)]
pub(in crate::engine::layout) struct ReadFrames {
    frames: Vec<Vec<LayoutRead>>,
}

impl ReadFrames {
    fn record(&mut self, read: LayoutRead) {
        if let Some(frame) = self.frames.last_mut() {
            frame.push(read);
        }
    }

    fn replay(&mut self, reads: &LayoutReads) {
        if let (Some(frame), Some(reads)) = (self.frames.last_mut(), reads.0.as_deref()) {
            frame.extend_from_slice(reads);
        }
    }
}

fn decoded_size(page: &Page, url: &str) -> Option<(u32, u32)> {
    page.images
        .get(url)
        .map(|image| (image.width, image.height))
}

fn scroll_offset_bits(node: &NodeRef) -> (u32, u32) {
    let (x, y) = node.scroll_offset.get();
    (x.to_bits(), y.to_bits())
}

fn color_bits(color: Color) -> (u8, u8, u8, u8) {
    (color.red, color.green, color.blue, color.alpha)
}

/// The activation target of the nearest ancestor link, for content promoted out of it.
fn inherited_link(page: &Page, node: &NodeRef) -> Option<(String, NodeId)> {
    std::iter::successors(Node::composed_parent(node), Node::composed_parent)
        .find(|ancestor| ancestor.tag_name() == Some("a"))
        .and_then(|anchor| {
            anchor
                .attr("href")
                .and_then(|href| resolve_url(&page.source_url, &href))
                .map(|url| (url, anchor.id()))
        })
}

/// `node`'s and its ancestors' background colors composited over white.
fn effective_backdrop(styles: &StyleSet, node: &NodeRef) -> Color {
    std::iter::successors(Some(node.clone()), Node::composed_parent)
        .map(|current| styles.get(&current).background_color)
        .filter(|color| color.alpha > 0)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .fold(Color::WHITE, |backdrop, color| {
            color.composite_over(backdrop)
        })
}

impl<'a, M: TextMeasurer> LayoutEngine<'a, M> {
    /// Opens a frame for a result that a cache may return later; pair with `end_reads`.
    pub(in crate::engine::layout) fn begin_reads(&self) {
        self.reads.borrow_mut().frames.push(Vec::new());
    }

    /// Closes the innermost frame, passes its reads to the enclosing one and returns them.
    pub(in crate::engine::layout) fn end_reads(&self) -> LayoutReads {
        let mut frames = self.reads.borrow_mut();
        let mut reads = frames
            .frames
            .pop()
            .expect("layout read frames are balanced");
        if reads.is_empty() {
            return LayoutReads::default();
        }
        let mut seen = HashSet::new();
        reads.retain(|read| seen.insert(read.clone()));
        if let Some(enclosing) = frames.frames.last_mut() {
            enclosing.extend_from_slice(&reads);
        }
        LayoutReads(Some(reads.into()))
    }

    /// Accounts a cache hit's reads to the result being computed now.
    pub(in crate::engine::layout) fn replay_reads(&self, reads: &LayoutReads) {
        self.reads.borrow_mut().replay(reads);
    }

    /// The URL an element displays (`Page::image_url`), recorded as a layout input.
    pub(in crate::engine::layout) fn image_url_for(&self, node: &NodeRef) -> Option<String> {
        let url = self.page.image_url(node);
        self.reads
            .borrow_mut()
            .record(LayoutRead::ImageUrl(ReadNode::new(node), url.clone()));
        url
    }

    /// A decoded image whose natural size this layout uses.
    pub(in crate::engine::layout) fn decoded_image(
        &self,
        url: &str,
    ) -> Option<&'a crate::engine::DecodedImage> {
        let image = self.page.images.get(url);
        self.reads.borrow_mut().record(LayoutRead::DecodedSize(
            url.to_string(),
            image.map(|image| (image.width, image.height)),
        ));
        image
    }

    /// A scroll container's current offset, recorded as a layout input.
    pub(in crate::engine::layout) fn scroll_offset(&self, node: &NodeRef) -> (f32, f32) {
        self.reads.borrow_mut().record(LayoutRead::ScrollOffset(
            ReadNode::new(node),
            scroll_offset_bits(node),
        ));
        node.scroll_offset.get()
    }

    /// The link a promoted inline formatting root inherits, recorded as a layout input.
    pub(in crate::engine::layout) fn recorded_inherited_link(
        &self,
        node: &NodeRef,
    ) -> Option<(String, NodeId)> {
        let link = inherited_link(self.page, node);
        self.reads
            .borrow_mut()
            .record(LayoutRead::InheritedLink(ReadNode::new(node), link.clone()));
        link
    }

    /// The backdrop a control composites over, recorded as a layout input.
    pub(in crate::engine::layout) fn effective_background_color(&self, node: &NodeRef) -> Color {
        let color = effective_backdrop(self.styles, node);
        self.reads
            .borrow_mut()
            .record(LayoutRead::Backdrop(ReadNode::new(node), color_bits(color)));
        color
    }

    /// `Page::layout_image_dimensions`, recorded as a layout input.
    pub(in crate::engine::layout) fn image_dimensions(&self, key: &str) -> Option<(u32, u32)> {
        let dimensions = self.page.layout_image_dimensions(key);
        self.reads
            .borrow_mut()
            .record(LayoutRead::LayoutDimensions(key.to_string(), dimensions));
        dimensions
    }
}
