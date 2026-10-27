//! Records the page inputs outside the DOM and computed styles that a cached layout result
//! read, so a later pass can tell whether the result still holds.
//
// For an unchanged DOM and style, these inputs are: which URL an element shows
// (`Page::image_url`), the natural dimensions of a decoded image or inline SVG, a scroll
// container's offset, and two values read from ancestors outside a subtree: the link a promoted
// inline formatting root inherits and the backdrop a form control composites.
//
// Every result that a cache may return later is computed inside a frame. Reads land in one log
// for the pass, and a frame is the part of the log written while it was open, so an enclosing
// frame includes everything its nested frames read. A cache hit appends the hit's recorded reads
// to the log. Only a result that is actually retained copies its frame's distinct reads.
use super::*;
use std::cell::Cell;
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
    /// Whether only the document tree and its URL decide this read.
    fn depends_only_on_document(&self) -> bool {
        matches!(self, Self::InheritedLink(..))
    }

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
pub(in crate::engine::layout) struct LayoutReads(Option<Rc<ReadSet>>);

#[derive(Debug)]
struct ReadSet {
    reads: Box<[LayoutRead]>,
    /// The document state at which the reads only it decides last held. A large retained
    /// subtree records thousands of them, and checking each walks its ancestors, so they are
    /// rechecked only after the document changes.
    document_held: Cell<DocumentStamp>,
}

/// A document's mutation version and a hash of the URL relative links resolve against, which
/// history navigation changes without mutating the document.
type DocumentStamp = (u64, u64);

fn document_stamp(page: &Page) -> DocumentStamp {
    use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};
    (
        page.dom.document.document_mutation_version(),
        BuildHasherDefault::<DefaultHasher>::default().hash_one(&page.source_url),
    )
}

/// Reads confirmed to hold during one layout pass. Layout does not change the document, so a
/// read that holds once holds for the rest of the pass. Nested cached results record the same
/// reads (a grid's probe and the retained content inside it each hold every read of that
/// content), and after any document change each read is otherwise rechecked, walking its
/// node's ancestors, once per result that recorded it.
pub(in crate::engine::layout) type HeldReads = HashSet<LayoutRead>;

impl LayoutReads {
    pub(in crate::engine::layout) fn hold(
        &self,
        page: &Page,
        styles: &StyleSet,
        held: &mut HeldReads,
    ) -> bool {
        let Some(set) = self.0.as_deref() else {
            return true;
        };
        let document = document_stamp(page);
        let document_held = set.document_held.get() == document;
        let hold = set.reads.iter().all(|read| {
            (document_held && read.depends_only_on_document())
                || held.contains(read)
                || (read.holds(page, styles) && held.insert(read.clone()))
        });
        if hold {
            set.document_held.set(document);
        }
        hold
    }

    pub(super) fn reads(&self) -> &[LayoutRead] {
        self.0.as_deref().map_or(&[], |set| &set.reads)
    }
}

#[derive(Default)]
pub(in crate::engine::layout) struct ReadFrames {
    log: Vec<LayoutRead>,
    frames: Vec<OpenFrame>,
}

#[derive(Clone, Copy)]
struct OpenFrame {
    start: usize,
    retained_items: usize,
}

/// A closed frame; its reads stay readable until the next outermost frame opens.
pub(in crate::engine::layout) struct ReadFrame {
    reads: std::ops::Range<usize>,
    /// The most display items any retained result inside the frame holds.
    pub(in crate::engine::layout) retained_items: usize,
}

impl ReadFrames {
    fn record(&mut self, read: LayoutRead) {
        if !self.frames.is_empty() {
            self.log.push(read);
        }
    }

    fn replay(&mut self, reads: &LayoutReads) {
        if !self.frames.is_empty() {
            self.log.extend_from_slice(reads.reads());
        }
    }

    fn note_retained(&mut self, items: usize) {
        if let Some(enclosing) = self.frames.last_mut() {
            enclosing.retained_items = enclosing.retained_items.max(items);
        }
    }
}

impl Page {
    /// Whether any layout of this page read the natural size of the image `url`. An image
    /// layout never read, such as one whose size is set without it (see
    /// `replaced_size_ignores_natural_size`), paints into the display item the last layout
    /// already made for it, so presenting its arrival needs no new layout. The set only grows:
    /// retained results replayed later skip the read but still depend on it.
    pub(crate) fn layout_read_image_size(&self, url: &str) -> bool {
        self.layout_image_reads.borrow().contains(url)
    }

    fn note_layout_image_read(&self, url: &str) {
        let mut reads = self.layout_image_reads.borrow_mut();
        if !reads.contains(url) {
            reads.insert(url.to_string());
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
        let mut reads = self.reads.borrow_mut();
        if reads.frames.is_empty() {
            reads.log.clear();
        }
        let start = reads.log.len();
        reads.frames.push(OpenFrame {
            start,
            retained_items: 0,
        });
    }

    /// Closes the innermost frame and returns its reads.
    pub(in crate::engine::layout) fn end_reads(&self) -> LayoutReads {
        let frame = self.close_reads();
        self.frame_reads(&frame)
    }

    /// Closes the innermost frame without copying its reads; see `frame_reads`.
    pub(in crate::engine::layout) fn close_reads(&self) -> ReadFrame {
        let mut reads = self.reads.borrow_mut();
        let frame = reads.frames.pop().expect("layout read frames are balanced");
        reads.note_retained(frame.retained_items);
        ReadFrame {
            reads: frame.start..reads.log.len(),
            retained_items: frame.retained_items,
        }
    }

    /// The distinct reads a closed frame made.
    pub(in crate::engine::layout) fn frame_reads(&self, frame: &ReadFrame) -> LayoutReads {
        self.distinct_reads(frame, |_| None)
    }

    /// The distinct reads a closed frame made while laying out `root`'s subtree, which a
    /// retained result validates by its DOM version. The link that inline content inside it
    /// inherits from above `root` is root's own inherited link, so those reads become one read
    /// of root's: rechecking one per inline formatting root after every DOM change cost a
    /// Wikipedia article ~9,000 ancestor walks per pass.
    pub(in crate::engine::layout) fn subtree_frame_reads(
        &self,
        frame: &ReadFrame,
        root: &NodeRef,
    ) -> LayoutReads {
        self.distinct_reads(frame, |read| {
            let LayoutRead::InheritedLink(node, link) = read else {
                return None;
            };
            let node = node.1.upgrade()?;
            for ancestor in
                std::iter::successors(Node::composed_parent(&node), Node::composed_parent)
            {
                if ancestor.tag_name() == Some("a") {
                    return None;
                }
                if ancestor.id() == root.id() {
                    return Some(LayoutRead::InheritedLink(ReadNode::new(root), link.clone()));
                }
            }
            None
        })
    }

    /// `frame`'s distinct reads, each replaced by `lift(read)` when that returns an
    /// equivalent read.
    fn distinct_reads(
        &self,
        frame: &ReadFrame,
        lift: impl Fn(&LayoutRead) -> Option<LayoutRead>,
    ) -> LayoutReads {
        let reads = self.reads.borrow();
        let log = &reads.log[frame.reads.clone()];
        if log.is_empty() {
            return LayoutReads::default();
        }
        let lifted = log
            .iter()
            .map(|read| {
                lift(read).map_or(std::borrow::Cow::Borrowed(read), std::borrow::Cow::Owned)
            })
            .collect::<Vec<_>>();
        let mut seen = HashSet::new();
        let distinct = lifted
            .iter()
            .filter(|read| seen.insert(&***read))
            .map(|read| read.as_ref().clone())
            .collect::<Vec<_>>();
        // Layout does not mutate the document, so every read made in this pass holds now.
        LayoutReads(Some(Rc::new(ReadSet {
            reads: distinct.into(),
            document_held: Cell::new(document_stamp(self.page)),
        })))
    }

    /// Tells the enclosing frame that a retained result of `items` display items lies inside it.
    pub(in crate::engine::layout) fn retained_items(&self, items: usize) {
        self.reads.borrow_mut().note_retained(items);
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
        self.page.note_layout_image_read(url);
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
        self.page.note_layout_image_read(key);
        self.reads
            .borrow_mut()
            .record(LayoutRead::LayoutDimensions(key.to_string(), dimensions));
        dimensions
    }
}
