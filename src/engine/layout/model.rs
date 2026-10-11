mod controls;
mod resize;
mod text;
pub use controls::{CheckableState, ControlKind, ControlSpec, FormSpec, SelectOption};
pub use text::{FontSpec, PositionedGlyph, ShapedText, TextMeasurer};
pub(super) mod fragments;
use super::*;
pub use fragments::{FragmentGeometry, TextCluster, TextGeometry};
pub use resize::ResizeBox;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RectF {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl RectF {
    pub fn inset(self, edges: ResolvedEdges) -> Self {
        Self {
            x: self.x + edges.left,
            y: self.y + edges.top,
            width: (self.width - edges.horizontal()).max(0.0),
            height: (self.height - edges.vertical()).max(0.0),
        }
    }

    pub fn right(self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(self) -> f32 {
        self.y + self.height
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DisplayItem {
    /// Renderer-local CSS paint-phase ownership; consumed by layout before presentation.
    PaintBoundary {
        kind: u8,
        entering: bool,
        level: i32,
        isolates: bool,
        node_id: Option<NodeId>,
    },
    /// Renderer-local anchors survive paint-group reordering and are stripped before IPC.
    NodeBoundary {
        node_id: NodeId,
        entering: bool,
    },
    BeginClip {
        bounds: RectF,
    },
    EndClip {
        bounds: RectF,
    },
    BeginOpacity {
        bounds: RectF,
        opacity: f32,
    },
    EndOpacity {
        bounds: RectF,
    },
    /// A retained 2D coordinate system. Bounds describe transformed ink, not normal flow.
    BeginTransform {
        bounds: RectF,
        matrix: crate::engine::affine::Affine2D,
    },
    EndTransform {
        bounds: RectF,
    },
    /// Renderer-local mask image layers, resolved before bounded IPC publication.
    BeginCssMask {
        bounds: RectF,
        layers: Vec<DisplayItem>,
    },
    BeginMask {
        bounds: RectF,
        url: String,
    },
    EndMask {
        bounds: RectF,
    },
    SolidRect {
        rect: RectF,
        color: Color,
        radius: f32,
    },
    BorderRect {
        rect: RectF,
        widths: [f32; 4],
        colors: [Color; 4],
        radius: f32,
    },
    Text {
        rect: RectF,
        text: String,
        font: FontSpec,
        color: Color,
        link: Option<String>,
        /// Stable renderer DOM target for hit-testable text such as anchor contents.
        node_id: Option<NodeId>,
        raster_run_id: u64,
        glyphs: Vec<PositionedGlyph>,
    },
    Image {
        rect: RectF,
        /// Replaced content may paint outside its element, but never outside its content box.
        clip: Option<RectF>,
        url: String,
        alt: String,
        tint: Option<Color>,
    },
    /// Renderer-local replaced element. The child document is composed before IPC.
    EmbeddedFrame {
        rect: RectF,
        node_id: NodeId,
    },
    BackgroundImage {
        clip_rect: RectF,
        tile_rect: RectF,
        url: String,
        repeat_x: bool,
        repeat_y: bool,
    },
    /// Renderer-local generated CSS image; rasterized through the existing image pipeline.
    CssGradient {
        clip_rect: RectF,
        tile_rect: RectF,
        gradient: crate::engine::css::images::gradient::geometry::ResolvedGradient,
        radius: f32,
        repeat_x: bool,
        repeat_y: bool,
    },
    /// Renderer-local ink only. It never changes scrollable overflow or hit boxes.
    CssBoxShadow {
        bounds: RectF,
        edge: RectF,
        offset: [f32; 2],
        blur: f32,
        spread: f32,
        radius: f32,
        color: Color,
        inset: bool,
    },
    Control(Box<ControlSpec>),
}

#[derive(Debug, Clone, Default)]
pub struct LayoutOutput {
    /// Exact local boxes and matrices for nested transformed geometry and hit tests.
    pub(crate) transformed: super::transforms::Geometry,
    /// Immutable when published to script; never serialized to the browser process.
    pub fragments: std::sync::Arc<FragmentGeometry>,
    pub(crate) sticky_offsets: HashMap<NodeId, (f32, f32)>,
    pub sticky_layers: Vec<super::StickyLayer>,
    /// Renderer-local scrollports; positions are unscrolled document coordinates.
    pub scroll_boxes: HashMap<NodeId, super::ScrollBox>,
    /// Native rectangular clip paths, retained for input hit testing.
    pub clip_paths: HashMap<NodeId, RectF>,
    /// Computed visibility and pointer-events eligibility for input hit testing.
    pub hit_excluded: HashSet<NodeId>,
    pub items: Vec<DisplayItem>,
    pub content_height: f32,
    pub background: Color,
    pub forms: HashMap<NodeId, FormSpec>,
    /// Renderer-local element border boxes used only by opt-in page diagnostics.
    pub node_bounds: HashMap<NodeId, RectF>,
    /// Untransformed used boxes, retained in the renderer for ResizeObserver (never sent over IPC).
    pub resize_boxes: HashMap<NodeId, ResizeBox>,
    /// Renderer-local back-to-front element order used by fallback hit testing. This stays next
    /// to layout rather than the wire presentation so input and paint use one stacking result.
    pub node_paint_order: Vec<NodeId>,
}

#[derive(Debug, Clone)]
pub(super) enum InlineAtom {
    BlockBox {
        node: NodeRef,
        height_basis: Option<f32>,
    },
    Text {
        text: String,
        font: FontSpec,
        color: Color,
        link: Option<String>,
        node_id: Option<NodeId>,
        source_node: Option<NodeId>,
        source_units: Vec<fragments::SourceUnit>,
        text_transform: TextTransform,
        visible: bool,
        preserve_space: bool,
        line_height: f32,
        normal_line_height: bool,
        no_wrap: bool,
    },
    Image {
        resize_box: ResizeBox,
        url: String,
        alt: String,
        tint: Option<Color>,
        node_id: NodeId,
        visible: bool,
        width: f32,
        height: f32,
        inset_x: f32,
        inset_y: f32,
        image_width: f32,
        image_height: f32,
        relative_offset: (f32, f32),
        transform: crate::engine::affine::Affine2D,
        opacity: f32,
    },
    Control {
        spec: Box<ControlSpec>,
        width: f32,
        height: f32,
        inset_x: f32,
        inset_y: f32,
        control_width: f32,
        control_height: f32,
        opacity: f32,
    },
    InlineBox {
        children: Vec<InlineAtom>,
        style: Box<ComputedStyle>,
        node_id: Option<NodeId>,
    },
    Placeholder {
        resize_box: ResizeBox,
        width: f32,
        height: f32,
        node_id: Option<NodeId>,
    },
    Break,
}

#[derive(Clone, Copy)]
pub(super) struct InlineContainingBlock {
    pub(super) width: f32,
    pub(super) height: Option<f32>,
}

pub(super) struct MeasuredAtom<'a> {
    pub(super) atom: &'a InlineAtom,
    pub(super) text: Option<&'a str>,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) content_height: f32,
    pub(super) no_wrap: bool,
    pub(super) break_before: bool,
}

#[derive(Debug, Clone)]
pub(super) struct CachedAtomMeasurement {
    pub(super) text_start: Option<usize>,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) content_height: f32,
    pub(super) no_wrap: bool,
    pub(super) break_before: bool,
}

impl CachedAtomMeasurement {
    pub(super) fn for_atom<'a>(&self, atom: &'a InlineAtom) -> MeasuredAtom<'a> {
        let text = match (atom, self.text_start) {
            (InlineAtom::Text { text, .. }, Some(start)) => text.get(start..),
            _ => None,
        };
        MeasuredAtom {
            atom,
            text,
            width: self.width,
            height: self.height,
            content_height: self.content_height,
            no_wrap: self.no_wrap,
            break_before: self.break_before,
        }
    }
}

impl From<&MeasuredAtom<'_>> for CachedAtomMeasurement {
    fn from(measured: &MeasuredAtom<'_>) -> Self {
        Self {
            text_start: measured.text.and_then(|measured_text| {
                let InlineAtom::Text { text, .. } = measured.atom else {
                    return None;
                };
                Some(text.len() - measured_text.len())
            }),
            width: measured.width,
            height: measured.height,
            content_height: measured.content_height,
            no_wrap: measured.no_wrap,
            break_before: measured.break_before,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct InlineBoxMetrics {
    pub(super) margin: ResolvedEdges,
    pub(super) border: ResolvedEdges,
    pub(super) padding: ResolvedEdges,
    pub(super) border_box_width: f32,
    pub(super) border_box_height: f32,
    pub(super) children_width: f32,
}

pub(super) struct GridItemPlacement {
    pub(super) node: NodeRef,
    pub(super) column: usize,
    pub(super) column_end: usize,
    pub(super) row: usize,
    pub(super) row_end: usize,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct GridAreaBounds {
    pub(super) row: usize,
    pub(super) row_end: usize,
    pub(super) column: usize,
    pub(super) column_end: usize,
}

pub(super) struct GridTemplateAreas {
    pub(super) row_count: usize,
    pub(super) column_count: usize,
    pub(super) areas: HashMap<String, GridAreaBounds>,
}

#[derive(Clone)]
pub(super) struct FlexItem {
    pub(super) node: Option<NodeRef>,
    pub(super) anonymous_atoms: Vec<InlineAtom>,
    pub(super) basis: f32,
    pub(super) grow: f32,
    pub(super) shrink: f32,
    pub(super) margin_start_auto: bool,
    pub(super) margin_end_auto: bool,
}
