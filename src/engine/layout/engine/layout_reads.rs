//! Records the page inputs outside the DOM and computed styles that a cached layout result
//! read, so a later pass can tell whether the result still holds.
//
// Image resources are the only such inputs that change between passes for an unchanged DOM
// and style: which URL an element shows (`Page::image_url`) and the natural dimensions of a
// decoded image or inline SVG. Every intrinsic computation that a cache may return later is
// computed inside a frame; reads made while a frame is open land in it, and a cache hit
// replays the hit's recorded reads into the enclosing frame. Closing a frame also passes its
// reads to the enclosing frame, so every cached result lists every read it depended on,
// however it was assembled.
use super::*;
use std::collections::HashSet;
use std::rc::Rc;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(in crate::engine::layout) enum LayoutRead {
    ImageUrl(NodeId, Option<String>),
    DecodedSize(String, Option<(u32, u32)>),
    LayoutDimensions(String, Option<(u32, u32)>),
}

impl LayoutRead {
    /// Whether `page` would still answer this read the same way.
    fn holds(&self, page: &Page) -> bool {
        match self {
            Self::ImageUrl(node, url) => page
                .dom
                .find_node(*node)
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
    pub(in crate::engine::layout) fn hold(&self, page: &Page) -> bool {
        self.0
            .as_deref()
            .is_none_or(|reads| reads.iter().all(|read| read.holds(page)))
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
            .record(LayoutRead::ImageUrl(node.id(), url.clone()));
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

    /// `Page::layout_image_dimensions`, recorded as a layout input.
    pub(in crate::engine::layout) fn image_dimensions(&self, key: &str) -> Option<(u32, u32)> {
        let dimensions = self.page.layout_image_dimensions(key);
        self.reads
            .borrow_mut()
            .record(LayoutRead::LayoutDimensions(key.to_string(), dimensions));
        dimensions
    }
}
