//! Typed, bounded page-diagnostic values carried inside a renderer presentation.

mod codec;

use crate::engine::RectF;
pub(super) use codec::{decode_diagnostics, encode_diagnostics};
use serde_json::{Value, json};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageDiagnostics {
    pub error: Option<String>,
    pub selectors: Vec<SelectorDiagnostics>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectorDiagnostics {
    pub selector: String,
    pub error: Option<String>,
    pub total_matches: u64,
    pub truncated: bool,
    pub matches: Vec<NodeDiagnostics>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NodeDiagnostics {
    pub node_id: u128,
    pub tag: Option<String>,
    pub id: Option<String>,
    pub class: Option<String>,
    pub child_count: u64,
    pub text_length: u64,
    pub shadow_root: Option<ShadowRootDiagnostics>,
    pub element_image: Option<ResourceDiagnostics>,
    pub style: StyleDiagnostics,
    pub control_rect: Option<RectF>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShadowRootDiagnostics {
    pub child_count: u64,
    pub descendant_count: u64,
    pub text_length: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StyleDiagnostics {
    pub display: String,
    pub position: String,
    pub float: String,
    pub visibility: bool,
    pub opacity: f32,
    pub overflow_hidden: bool,
    pub list_style_type: String,
    pub width: String,
    pub height: String,
    pub min_width: String,
    pub max_width: String,
    pub min_height: String,
    pub max_height: String,
    pub background_color: String,
    pub background_image: Option<ResourceDiagnostics>,
    pub mask_image: Option<ResourceDiagnostics>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResourceDiagnostics {
    pub kind: String,
    pub url: Option<String>,
    pub data_prefix: Option<String>,
    pub decoded: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub nontransparent_pixels: Option<u64>,
    pub paint_rects: Vec<RectF>,
    pub control_rects: Vec<RectF>,
}

impl PageDiagnostics {
    pub fn to_json(&self) -> Value {
        if let Some(error) = &self.error {
            return json!({ "error": error });
        }
        Value::Array(
            self.selectors
                .iter()
                .map(SelectorDiagnostics::to_json)
                .collect(),
        )
    }
}

impl SelectorDiagnostics {
    fn to_json(&self) -> Value {
        if let Some(error) = &self.error {
            return json!({ "selector": self.selector, "error": error });
        }
        json!({
            "selector": self.selector,
            "total_matches": self.total_matches,
            "truncated": self.truncated,
            "matches": self.matches.iter().map(NodeDiagnostics::to_json).collect::<Vec<_>>(),
        })
    }
}

impl NodeDiagnostics {
    fn to_json(&self) -> Value {
        json!({
            "node_id": format!("{:032x}", self.node_id),
            "tag": self.tag,
            "id": self.id,
            "class": self.class,
            "child_count": self.child_count,
            "text_length": self.text_length,
            "shadow_root": self.shadow_root.as_ref().map(ShadowRootDiagnostics::to_json),
            "element_image": self.element_image.as_ref().map(ResourceDiagnostics::to_json),
            "style": self.style.to_json(),
            "control_rect": self.control_rect.map(rect_value),
        })
    }
}

impl ShadowRootDiagnostics {
    fn to_json(&self) -> Value {
        json!({
            "child_count": self.child_count,
            "descendant_count": self.descendant_count,
            "text_length": self.text_length,
        })
    }
}

impl StyleDiagnostics {
    fn to_json(&self) -> Value {
        json!({
            "display": self.display,
            "position": self.position,
            "float": self.float,
            "visibility": self.visibility,
            "opacity": self.opacity,
            "overflow_hidden": self.overflow_hidden,
            "list_style_type": self.list_style_type,
            "width": self.width,
            "height": self.height,
            "min_width": self.min_width,
            "max_width": self.max_width,
            "min_height": self.min_height,
            "max_height": self.max_height,
            "background_color": self.background_color,
            "background_image": self.background_image.as_ref().map(ResourceDiagnostics::to_json),
            "mask_image": self.mask_image.as_ref().map(ResourceDiagnostics::to_json),
        })
    }
}

impl ResourceDiagnostics {
    fn to_json(&self) -> Value {
        json!({
            "kind": self.kind,
            "url": self.url,
            "data_prefix": self.data_prefix,
            "decoded": self.decoded,
            "width": self.width,
            "height": self.height,
            "nontransparent_pixels": self.nontransparent_pixels,
            "paint_rects": self.paint_rects.iter().copied().map(rect_value).collect::<Vec<_>>(),
            "control_rects": self.control_rects.iter().copied().map(rect_value).collect::<Vec<_>>(),
        })
    }
}

fn rect_value(rect: RectF) -> Value {
    json!({ "x": rect.x, "y": rect.y, "width": rect.width, "height": rect.height })
}
