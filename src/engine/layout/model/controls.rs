//! Form-control geometry and immutable presentation state.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    Text,
    TextArea,
    Password,
    Search,
    File,
    Select,
    Submit,
    Button,
    Reset,
    Checkbox,
    Radio,
}

impl ControlKind {
    pub fn is_checkable(self) -> bool {
        matches!(self, Self::Checkbox | Self::Radio)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckableState {
    pub checked: bool,
    pub indeterminate: bool,
    pub disabled: bool,
    pub appearance: bool,
    pub accent: Option<Color>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ControlSpec {
    pub checkable: Option<CheckableState>,
    /// The retained descendants paint this control; no native child may cover them.
    pub authored_content: bool,
    pub node_id: NodeId,
    pub rect: RectF,
    pub kind: ControlKind,
    pub name: String,
    pub value: String,
    pub label: String,
    pub options: Vec<SelectOption>,
    pub selected_index: i64,
    pub placeholder: String,
    pub form_id: Option<NodeId>,
    pub background_color: Color,
    pub text_color: Color,
    pub placeholder_color: Color,
    pub border_colors: [Color; 4],
    pub border_width: [f32; 4],
    pub border_radius: f32,
    pub padding: [f32; 4],
    pub font: FontSpec,
    pub icon_url: Option<String>,
    pub icon_width: f32,
    pub icon_height: f32,
    /// Interactively reported invalid state with its message for feedback.
    pub invalid: bool,
    pub validation_message: String,
}

impl ControlSpec {
    pub fn uses_native_window(&self) -> bool {
        !self.authored_content && !self.kind.is_checkable()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FormSpec {
    pub node_id: NodeId,
    pub action: String,
    pub method: String,
    pub hidden_fields: Vec<(String, String)>,
}
