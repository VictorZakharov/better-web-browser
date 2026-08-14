use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RectF {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl RectF {
    pub fn right(self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(self) -> f32 {
        self.y + self.height
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FontSpec {
    pub family: String,
    pub size: f32,
    pub weight: u16,
    pub italic: bool,
    pub underline: bool,
}

impl FontSpec {
    pub(super) fn from_style(style: &ComputedStyle) -> Self {
        Self {
            family: style.font_family.clone(),
            size: style.font_size,
            weight: style.font_weight,
            italic: style.italic,
            underline: style.text_decoration_underline,
        }
    }
}

pub trait TextMeasurer {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    Text,
    TextArea,
    Password,
    Search,
    Select,
    Submit,
    Button,
    Reset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ControlSpec {
    pub node_id: NodeId,
    pub rect: RectF,
    pub kind: ControlKind,
    pub name: String,
    pub value: String,
    pub label: String,
    pub options: Vec<SelectOption>,
    pub selected_index: usize,
    pub placeholder: String,
    pub form_id: Option<NodeId>,
    pub background_color: Color,
    pub text_color: Color,
    pub border_color: Color,
    pub border_width: [f32; 4],
    pub border_radius: f32,
    pub padding: [f32; 4],
    pub font: FontSpec,
    pub icon_url: Option<String>,
    pub icon_width: f32,
    pub icon_height: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FormSpec {
    pub node_id: NodeId,
    pub action: String,
    pub method: String,
    pub hidden_fields: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DisplayItem {
    SolidRect {
        rect: RectF,
        color: Color,
        radius: f32,
    },
    BorderRect {
        rect: RectF,
        widths: [f32; 4],
        color: Color,
        radius: f32,
    },
    Text {
        rect: RectF,
        text: String,
        font: FontSpec,
        color: Color,
        link: Option<String>,
    },
    Image {
        rect: RectF,
        url: String,
        alt: String,
        tint: Option<Color>,
    },
    BackgroundImage {
        clip_rect: RectF,
        tile_rect: RectF,
        url: String,
        repeat_x: bool,
        repeat_y: bool,
    },
    Control(Box<ControlSpec>),
}

#[derive(Debug, Default)]
pub struct LayoutOutput {
    pub items: Vec<DisplayItem>,
    pub content_height: f32,
    pub background: Color,
    pub forms: HashMap<NodeId, FormSpec>,
}

#[derive(Debug)]
pub(super) enum InlineAtom {
    Text {
        text: String,
        font: FontSpec,
        color: Color,
        link: Option<String>,
        line_height: f32,
        no_wrap: bool,
    },
    Image {
        url: String,
        alt: String,
        tint: Option<Color>,
        width: f32,
        height: f32,
        inset_x: f32,
        inset_y: f32,
        image_width: f32,
        image_height: f32,
    },
    Control {
        spec: Box<ControlSpec>,
        width: f32,
        height: f32,
        inset_x: f32,
        inset_y: f32,
        control_width: f32,
        control_height: f32,
    },
    InlineBox {
        children: Vec<InlineAtom>,
        style: Box<ComputedStyle>,
    },
    Placeholder {
        width: f32,
        height: f32,
    },
    Break,
}

pub(super) struct MeasuredAtom<'a> {
    pub(super) atom: &'a InlineAtom,
    pub(super) text: Option<&'a str>,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) no_wrap: bool,
    pub(super) break_before: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct CachedAtomMeasurement {
    pub(super) text_start: Option<usize>,
    pub(super) width: f32,
    pub(super) height: f32,
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

#[derive(Debug, Clone)]
pub(super) enum GridTrack {
    Auto,
    Fixed(Length),
    Fraction(f32),
    MinMax(Box<GridTrack>, Box<GridTrack>),
}

pub(super) struct GridItemPlacement {
    pub(super) node: NodeRef,
    pub(super) column: usize,
    pub(super) column_end: usize,
    pub(super) row: usize,
    pub(super) row_end: usize,
}

#[derive(Clone)]
pub(super) struct FlexItem {
    pub(super) node: NodeRef,
    pub(super) basis: f32,
    pub(super) grow: f32,
    pub(super) shrink: f32,
    pub(super) margin_start_auto: bool,
    pub(super) margin_end_auto: bool,
}
