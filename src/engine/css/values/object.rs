//! CSS Images 3 object sizing and positioning for replaced content.

use super::Length;
use crate::engine::css::parse_length;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ObjectFit {
    #[default]
    Fill,
    Contain,
    Cover,
    None,
    ScaleDown,
}

impl ObjectFit {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "fill" => Some(Self::Fill),
            "contain" => Some(Self::Contain),
            "cover" => Some(Self::Cover),
            "none" => Some(Self::None),
            "scale-down" => Some(Self::ScaleDown),
            _ => None,
        }
    }

    pub(crate) fn css_text(self) -> &'static str {
        match self {
            Self::Fill => "fill",
            Self::Contain => "contain",
            Self::Cover => "cover",
            Self::None => "none",
            Self::ScaleDown => "scale-down",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum PositionAxis {
    /// A percentage is relative to the leftover space, not to the element's size.
    Value(Length),
    /// Three-/four-component syntax: e.g. `right 10px bottom 20%`.
    EdgeOffset { end: bool, offset: Length },
}

impl PositionAxis {
    fn resolve(self, box_size: f32, object_size: f32, font_size: f32) -> f32 {
        let free = box_size - object_size;
        match self {
            Self::Value(value) => value.resolve(free, font_size).unwrap_or(0.0),
            Self::EdgeOffset { end, offset } => {
                let offset = offset.resolve(box_size, font_size).unwrap_or(0.0);
                if end { free - offset } else { offset }
            }
        }
    }

    fn resolve_relative_units(self, width: f32, height: f32, root_font_size: f32) -> Self {
        let resolve = |value: Length| {
            value
                .resolve_root_font_units(root_font_size)
                .resolve_viewport_units(width, height)
        };
        match self {
            Self::Value(value) => Self::Value(resolve(value)),
            Self::EdgeOffset { end, offset } => Self::EdgeOffset {
                end,
                offset: resolve(offset),
            },
        }
    }

    fn css_text(self, start: &str, end: &str, font_size: f32) -> String {
        match self {
            Self::Value(value) => super::super::cssom::serialize_scroll_spacing(value, font_size),
            Self::EdgeOffset { end: false, offset } => {
                format!(
                    "{start} {}",
                    super::super::cssom::serialize_scroll_spacing(offset, font_size)
                )
            }
            Self::EdgeOffset { end: true, offset } => {
                format!(
                    "{end} {}",
                    super::super::cssom::serialize_scroll_spacing(offset, font_size)
                )
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectPosition {
    horizontal: PositionAxis,
    vertical: PositionAxis,
}

impl Default for ObjectPosition {
    fn default() -> Self {
        Self {
            horizontal: PositionAxis::Value(Length::Percent(50.0)),
            vertical: PositionAxis::Value(Length::Percent(50.0)),
        }
    }
}

impl ObjectPosition {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        let tokens = position_tokens(value)?;
        match tokens.as_slice() {
            [only] => Self::one(only),
            [first, second] => Self::two(first, second),
            [_, _, _] | [_, _, _, _] => Self::edge_offsets(&tokens),
            _ => None,
        }
    }

    pub(crate) fn resolve(
        self,
        box_width: f32,
        box_height: f32,
        object_width: f32,
        object_height: f32,
        font_size: f32,
    ) -> (f32, f32) {
        (
            self.horizontal.resolve(box_width, object_width, font_size),
            self.vertical.resolve(box_height, object_height, font_size),
        )
    }

    pub(crate) fn resolve_relative_units(
        self,
        width: f32,
        height: f32,
        root_font_size: f32,
    ) -> Self {
        Self {
            horizontal: self
                .horizontal
                .resolve_relative_units(width, height, root_font_size),
            vertical: self
                .vertical
                .resolve_relative_units(width, height, root_font_size),
        }
    }

    pub(crate) fn css_text(self, font_size: f32) -> String {
        format!(
            "{} {}",
            self.horizontal.css_text("left", "right", font_size),
            self.vertical.css_text("top", "bottom", font_size)
        )
    }

    fn one(token: &str) -> Option<Self> {
        let center = PositionAxis::Value(Length::Percent(50.0));
        if token == "center" {
            return Some(Self::default());
        }
        if let Some(horizontal) = horizontal_keyword(token) {
            return Some(Self {
                horizontal,
                vertical: center,
            });
        }
        if let Some(vertical) = vertical_keyword(token) {
            return Some(Self {
                horizontal: center,
                vertical,
            });
        }
        Some(Self {
            horizontal: PositionAxis::Value(parse_length(token)?),
            vertical: center,
        })
    }

    fn two(first: &str, second: &str) -> Option<Self> {
        let center = PositionAxis::Value(Length::Percent(50.0));
        if first == "center" {
            return if let Some(horizontal) = horizontal_keyword(second) {
                Some(Self {
                    horizontal,
                    vertical: center,
                })
            } else {
                Some(Self {
                    horizontal: center,
                    vertical: vertical_keyword(second)
                        .or_else(|| parse_length(second).map(PositionAxis::Value))?,
                })
            };
        }
        if second == "center" {
            return if let Some(vertical) = vertical_keyword(first) {
                Some(Self {
                    horizontal: center,
                    vertical,
                })
            } else {
                Some(Self {
                    horizontal: horizontal_keyword(first)
                        .or_else(|| parse_length(first).map(PositionAxis::Value))?,
                    vertical: center,
                })
            };
        }
        if let Some(vertical) = vertical_keyword(first) {
            let horizontal = horizontal_keyword(second)
                .or_else(|| parse_length(second).map(PositionAxis::Value))?;
            return Some(Self {
                horizontal,
                vertical,
            });
        }
        if let Some(horizontal) = horizontal_keyword(first) {
            let vertical = vertical_keyword(second)
                .or_else(|| parse_length(second).map(PositionAxis::Value))?;
            return Some(Self {
                horizontal,
                vertical,
            });
        }
        let horizontal = PositionAxis::Value(parse_length(first)?);
        Some(Self {
            horizontal,
            vertical: vertical_keyword(second)
                .or_else(|| parse_length(second).map(PositionAxis::Value))?,
        })
    }

    fn edge_offsets(tokens: &[&str]) -> Option<Self> {
        let mut horizontal = None;
        let mut vertical = None;
        let mut index = 0;
        let mut offset_count = 0;
        let mut center_seen = false;
        while index < tokens.len() {
            let keyword = tokens[index];
            if keyword == "center" {
                if center_seen {
                    return None;
                }
                center_seen = true;
                index += 1;
                continue;
            }
            let axis = if horizontal_keyword(keyword).is_some() {
                &mut horizontal
            } else if vertical_keyword(keyword).is_some() {
                &mut vertical
            } else {
                return None;
            };
            if axis.is_some() {
                return None;
            }
            if index + 1 < tokens.len()
                && let Some(offset) = parse_length(tokens[index + 1])
            {
                *axis = Some(PositionAxis::EdgeOffset {
                    end: matches!(keyword, "right" | "bottom"),
                    offset,
                });
                offset_count += 1;
                index += 2;
                continue;
            }
            *axis = horizontal_keyword(keyword).or_else(|| vertical_keyword(keyword));
            index += 1;
        }
        if center_seen {
            let center = PositionAxis::Value(Length::Percent(50.0));
            if horizontal.is_none() {
                horizontal = Some(center);
            } else if vertical.is_none() {
                vertical = Some(center);
            } else {
                return None;
            }
        }
        (offset_count > 0).then_some(Self {
            horizontal: horizontal?,
            vertical: vertical?,
        })
    }
}

fn horizontal_keyword(value: &str) -> Option<PositionAxis> {
    match value {
        "left" => Some(PositionAxis::Value(Length::Percent(0.0))),
        "right" => Some(PositionAxis::Value(Length::Percent(100.0))),
        _ => None,
    }
}

fn vertical_keyword(value: &str) -> Option<PositionAxis> {
    match value {
        "top" => Some(PositionAxis::Value(Length::Percent(0.0))),
        "bottom" => Some(PositionAxis::Value(Length::Percent(100.0))),
        _ => None,
    }
}

/// Split CSS whitespace only outside functions, preserving `calc(50% + 1em)`.
fn position_tokens(value: &str) -> Option<Vec<&str>> {
    let mut depth = 0;
    let mut start = None;
    let mut tokens = Vec::new();
    for (index, character) in value.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            ',' | '/' if depth == 0 => return None,
            _ => {}
        }
        if character.is_whitespace() && depth == 0 {
            if let Some(start) = start.take() {
                tokens.push(&value[start..index]);
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if depth != 0 {
        return None;
    }
    if let Some(start) = start {
        tokens.push(&value[start..]);
    }
    (1..=4).contains(&tokens.len()).then_some(tokens)
}
