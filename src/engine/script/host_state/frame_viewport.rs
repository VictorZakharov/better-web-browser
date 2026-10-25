//! A nested document's viewport follows its iframe's current content box.
use super::*;

/// The iframe element, in its parent document, whose content box is this document's viewport.
pub(in crate::engine::script) struct FrameEmbedding {
    parent: std::rc::Weak<RefCell<HostState>>,
    element: NodeRef,
    /// The parent's DOM version and viewport when the content box was last read. The box
    /// cannot change while both stay the same, so reads skip the parent flush.
    synced: std::cell::Cell<Option<(u64, u32, u32)>>,
}

impl FrameEmbedding {
    pub(in crate::engine::script) fn new(
        parent: std::rc::Weak<RefCell<HostState>>,
        element: NodeRef,
    ) -> Self {
        Self {
            parent,
            element,
            synced: Default::default(),
        }
    }
}

const CONTENT_EDGES: [&str; 8] = [
    "border-left-width",
    "padding-left",
    "padding-right",
    "border-right-width",
    "border-top-width",
    "padding-top",
    "padding-bottom",
    "border-bottom-width",
];

impl HostState {
    /// Media queries in a nested document evaluate against its viewport, the iframe's content
    /// box, which the parent document's layout determines. A style read flushes that layout
    /// first, as it would within one document, so a script-driven iframe resize is visible
    /// synchronously instead of after the next composed paint.
    /// https://drafts.csswg.org/mediaqueries-4/#mf-dimensions
    pub(in crate::engine::script) fn sync_embedded_viewport(&mut self) {
        let Some((width, height)) = self.embedding_content_box() else {
            return;
        };
        let environment = self.media_environment.with_viewport(width, height);
        let current = self.media_environment;
        // Sub-pixel differences from the composed paint rect are not a resize.
        if (environment.viewport_width - current.viewport_width).abs() < 0.01
            && (environment.viewport_height - current.viewport_height).abs() < 0.01
        {
            return;
        }
        self.media_environment = environment;
        self.layout_viewport_width = width;
        self.layout_viewport_height = height;
        self.css_animation_revision = self.css_animation_revision.wrapping_add(1);
        self.computed_styles = None;
        self.offset_parent_styles = None;
        self.layout_geometry_initialized = false;
        self.pending_invalidation
            .record(&self.document, None, MutationKind::Viewport);
        self.pending_layout_invalidation
            .record(&self.document, None, MutationKind::Viewport);
    }

    fn embedding_content_box(&self) -> Option<(f32, f32)> {
        let embedding = self.frame_embedding.as_ref()?;
        let parent = embedding.parent.upgrade()?;
        // A parent already inside a host call is mid-flush; keep the published viewport.
        let mut parent = parent.try_borrow_mut().ok()?;
        let key = (
            parent.document.subtree_mutation_version(),
            parent.media_environment.viewport_width.to_bits(),
            parent.media_environment.viewport_height.to_bits(),
        );
        if embedding.synced.get() == Some(key) || !parent.is_connected(&embedding.element) {
            return None;
        }
        parent.flush_layout_if_needed();
        // Until the parent has laid the iframe out, keep retrying on later reads.
        let rect = parent
            .layout_geometry
            .get(&embedding.element.id())
            .copied()?;
        embedding.synced.set(Some(key));
        let edges = CONTENT_EDGES.map(String::from);
        let edges = parent.computed_style_properties(&embedding.element, &edges);
        let edge = |range: std::ops::Range<usize>| -> f32 {
            edges[range]
                .iter()
                .map(|value| value.trim_end_matches("px").parse::<f32>().unwrap_or(0.0))
                .sum()
        };
        Some((
            (rect.width - edge(0..4)).max(0.0),
            (rect.height - edge(4..8)).max(0.0),
        ))
    }
}

/// Lays out a nested document's page in its viewport with the renderer's text system.
/// The flag requests display items for a hit-test snapshot; otherwise geometry suffices.
pub(crate) type FrameLayoutService =
    Rc<dyn Fn(&crate::engine::Page, f32, f32, bool) -> Option<crate::engine::LayoutOutput>>;

impl HostState {
    /// A nested document has no renderer-owned layout snapshot of its own; its paint layout is
    /// composed into the parent's and is absent for a zero-size iframe. Geometry reads such as
    /// a rendered image's `height` lay the document out on demand in its current viewport.
    pub(super) fn flush_frame_layout(&mut self, hit_test_requested: bool) {
        let Some(service) = self.frame_layout.clone() else {
            return;
        };
        if self.frame_embedding.is_none() {
            return;
        }
        self.sync_embedded_viewport();
        let (width, height) = (self.layout_viewport_width, self.layout_viewport_height);
        let mut page = crate::engine::Page::from_frame_document(
            self.document.clone(),
            &self.script_base_url(),
            self.stylesheet_sources.clone(),
            self.quirks_mode,
            self.media_environment,
        );
        page.images.extend(
            self.element_images
                .decoded()
                .map(|(url, image)| (url.clone(), image.clone())),
        );
        page.set_layout_viewport(width, height);
        let Some(layout) = service(&page, width, height, hit_test_requested) else {
            return;
        };
        self.adopt_frame_layout(&layout);
        if hit_test_requested {
            self.hit_test_snapshot =
                crate::engine::layout::HitTestSnapshot::from_layout(&layout, &self.document);
            self.hit_test_geometry_version = Some(self.layout_geometry_version);
        }
        self.geometry_scroll_offset = self.document.scroll_offset.get();
        self.geometry_scroll_dirty = false;
    }

    pub(in crate::engine::script) fn adopt_frame_layout(
        &mut self,
        layout: &crate::engine::LayoutOutput,
    ) {
        self.layout_geometry.clone_from(&layout.node_bounds);
        self.layout_fragments = layout.fragments.clone();
        self.scroll_boxes.clone_from(&layout.scroll_boxes);
        self.resize_boxes.clone_from(&layout.resize_boxes);
        self.sticky_offsets.clone_from(&layout.sticky_offsets);
        self.layout_content_height = layout.content_height;
        self.layout_geometry_version = self.document.subtree_mutation_version();
        self.layout_geometry_initialized = true;
        self.pending_layout_invalidation
            .acknowledge_published_geometry();
    }
}
