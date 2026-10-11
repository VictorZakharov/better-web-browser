//! CSS Transforms 1 §5: a reference-box position followed by an optional Z length.
//! Reuse the existing CSS position resolver, but do not admit its edge-offset grammar.
use super::*;
use crate::engine::css::syntax::ident;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TransformOrigin {
    position: ObjectPosition,
    z: Length,
}

impl Default for TransformOrigin {
    fn default() -> Self {
        Self {
            position: ObjectPosition::default(),
            z: Length::Px(0.0),
        }
    }
}

impl TransformOrigin {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        if value.len() > 16_384 {
            return None;
        }
        let parts = borrowed_components(value)?;
        if !(1..=3).contains(&parts.len()) {
            return None;
        }
        let mut tokens = Vec::new();
        for value in parts.iter().take(2) {
            if let Some(keyword) = ident(value) {
                let keyword = keyword.to_ascii_lowercase();
                if !matches!(
                    keyword.as_str(),
                    "left" | "right" | "top" | "bottom" | "center"
                ) {
                    return None;
                }
                tokens.push(keyword);
            } else {
                let length = parse_length(value)?;
                if length == Length::Auto {
                    return None;
                }
                tokens.push(serialize_length(length));
            }
        }
        // Transform-origin's two-value syntax does not allow `top 10px`:
        // vertical-first ordering is reserved for two keywords.
        if tokens.len() == 2
            && matches!(tokens[0].as_str(), "top" | "bottom")
            && !matches!(tokens[1].as_str(), "left" | "right" | "center")
        {
            return None;
        }
        let z = if let Some(value) = parts.get(2) {
            let length = parse_length(value)?;
            if length == Length::Auto || length.has_percentage() {
                return None;
            }
            length
        } else {
            Length::Px(0.0)
        };
        Some(Self {
            position: ObjectPosition::parse(&tokens.join(" "))?,
            z,
        })
    }

    pub(crate) fn resolve(&self, width: f32, height: f32, font: f32) -> (f32, f32) {
        self.position.clone().resolve(width, height, 0.0, 0.0, font)
    }

    pub(crate) fn css_text(&self, font: f32) -> String {
        let xy = self.position.clone().css_text(font);
        if self.z == Length::Px(0.0) {
            xy
        } else {
            format!("{xy} {}", serialize_length(self.z.clone()))
        }
    }

    pub(crate) fn apply_zoom(&mut self, zoom: f32) {
        self.position.apply_zoom(zoom);
        self.z = values::zoom::length(&self.z, zoom);
    }

    pub(crate) fn resolve_relative_units(&mut self, width: f32, height: f32, root: f32) {
        self.position = self
            .position
            .clone()
            .resolve_relative_units(width, height, root);
        self.z = self
            .z
            .clone()
            .resolve_root_font_units(root)
            .resolve_viewport_units(width, height);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum TransformBox {
    #[default]
    View,
    Border,
    Content,
    Fill,
    Stroke,
}

impl TransformBox {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match ident(value)?.to_ascii_lowercase().as_str() {
            "view-box" => Some(Self::View),
            "border-box" => Some(Self::Border),
            "content-box" => Some(Self::Content),
            "fill-box" => Some(Self::Fill),
            "stroke-box" => Some(Self::Stroke),
            _ => None,
        }
    }
    pub(crate) fn css_text(self) -> &'static str {
        match self {
            Self::View => "view-box",
            Self::Border => "border-box",
            Self::Content => "content-box",
            Self::Fill => "fill-box",
            Self::Stroke => "stroke-box",
        }
    }
    pub(crate) fn uses_content(self) -> bool {
        matches!(self, Self::Content | Self::Fill)
    }
}

#[cfg(test)]
mod tests;
