//! Snapshot child document paint inputs without mixing their viewport geometry with the parent.

use super::*;

type FrameGeometryPublisher = Box<dyn Fn(&crate::engine::LayoutOutput, RectF)>;

pub(crate) struct FramePaintSnapshot {
    pub element: NodeId,
    pub document: NodeId,
    pub dom: NodeRef,
    pub url: String,
    pub stylesheets: Vec<crate::engine::css::StylesheetSource>,
    pub media_environment: crate::engine::MediaEnvironment,
    pub quirks_mode: bool,
    pub images: HashMap<String, crate::engine::DecodedImage>,
    pub canvas_updates: HashSet<String>,
    pub children: Vec<FramePaintSnapshot>,
    /// Publish the exact child layout used for paint to its script realm. A child
    /// document has its own viewport and cannot use the parent document's boxes.
    pub publish_geometry: FrameGeometryPublisher,
}

impl ScriptRuntime {
    pub(crate) fn frame_paint_snapshots(&mut self) -> Vec<FramePaintSnapshot> {
        self.frame_paint_snapshots_bounded(0)
    }

    fn frame_paint_snapshots_bounded(&mut self, depth: usize) -> Vec<FramePaintSnapshot> {
        if depth >= 8 {
            return Vec::new();
        }
        self.sync_child_runtimes();
        let document = self.host.borrow().document.id();
        let elements = self
            .context
            .as_ref()
            .map(|context| context.child_frame_elements(document))
            .unwrap_or_default();
        let Some(frames) = self.frames.as_mut() else {
            return Vec::new();
        };
        elements
            .into_iter()
            .filter_map(|(element, id)| {
                frames.sync_canvas(id);
                let (images, canvas_updates) = frames
                    .images
                    .get(&id)
                    .map(|state| (state.decoded.clone(), state.canvas_updates.clone()))
                    .unwrap_or_default();
                let child = frames.children.get_mut(&id)?;
                let snapshot = {
                    let host = child.host.borrow();
                    FramePaintSnapshot {
                        element,
                        document: id,
                        dom: host.document.clone(),
                        url: host.script_base_url(),
                        stylesheets: host.stylesheet_sources.clone(),
                        media_environment: host.media_environment,
                        quirks_mode: host.quirks_mode,
                        images,
                        canvas_updates,
                        children: Vec::new(),
                        publish_geometry: {
                            let host = Rc::clone(&child.host);
                            Box::new(move |layout, viewport| {
                                let mut host = host.borrow_mut();
                                host.media_environment = host
                                    .media_environment
                                    .with_viewport(viewport.width, viewport.height);
                                host.layout_viewport_width = viewport.width;
                                host.layout_viewport_height = viewport.height;
                                host.embedding_rect = Some(viewport);
                                host.frame_layout_pending = false;
                                host.layout_geometry.clone_from(&layout.node_bounds);
                                host.layout_fragments = layout.fragments.clone();
                                host.scroll_boxes.clone_from(&layout.scroll_boxes);
                                host.resize_boxes.clone_from(&layout.resize_boxes);
                                host.sticky_offsets.clone_from(&layout.sticky_offsets);
                                host.layout_content_height = layout.content_height;
                                host.layout_geometry_version =
                                    host.document.subtree_mutation_version();
                                host.layout_geometry_initialized = true;
                                host.pending_layout_invalidation
                                    .acknowledge_published_geometry();
                            })
                        },
                    }
                };
                Some(FramePaintSnapshot {
                    children: child.frame_paint_snapshots_bounded(depth + 1),
                    ..snapshot
                })
            })
            .collect()
    }
}
