//! Rectangular CSS basic-shape clipping. Unsupported shapes are not advertised.
//! https://www.w3.org/TR/css-masking-1/#the-clip-path

use super::{Edges, Length, ResolvedEdges, parse_length, scroll_spacing};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) enum ClipPath {
    #[default]
    None,
    Inset(Edges),
}

impl ClipPath {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        let value = value.trim().to_ascii_lowercase();
        if value == "none" {
            return Some(Self::None);
        }
        let inside = value.strip_prefix("inset(")?.strip_suffix(')')?;
        let parts = scroll_spacing::split_components(inside)?;
        if !(1..=4).contains(&parts.len()) {
            return None;
        }
        let lengths = parts
            .iter()
            .map(|part| parse_length(part).filter(|length| *length != Length::Auto))
            .collect::<Option<Vec<_>>>()?;
        Some(Self::Inset(scroll_spacing::expand_edges(&lengths)))
    }

    pub(crate) fn resolve_relative_units(
        &mut self,
        viewport_width: f32,
        viewport_height: f32,
        root_font_size: f32,
    ) {
        if let Self::Inset(edges) = self {
            *edges = edges.resolve_relative_units(viewport_width, viewport_height, root_font_size);
        }
    }

    pub(crate) fn inset(self, width: f32, height: f32, font_size: f32) -> Option<ResolvedEdges> {
        let Self::Inset(edges) = self else {
            return None;
        };
        Some(ResolvedEdges {
            top: edges.top.resolve(height, font_size)?,
            right: edges.right.resolve(width, font_size)?,
            bottom: edges.bottom.resolve(height, font_size)?,
            left: edges.left.resolve(width, font_size)?,
        })
    }

    pub(crate) fn serialize(self, font_size: f32) -> String {
        let Self::Inset(edges) = self else {
            return "none".into();
        };
        let parts = [edges.top, edges.right, edges.bottom, edges.left]
            .map(|length| super::cssom::serialize_scroll_spacing(length, font_size));
        format!("inset({})", parts.join(" "))
    }

    pub(crate) fn is_none(self) -> bool {
        matches!(self, Self::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{css::StyleSet, dom};

    #[test]
    fn inset_shorthand_parses_units_and_rejects_unsupported_shapes() {
        let clip = ClipPath::parse("inset(10% calc(1px + 2px) 4px)").unwrap();
        let offsets = clip.inset(100.0, 200.0, 16.0).unwrap();
        assert_eq!(
            (offsets.top, offsets.right, offsets.bottom, offsets.left),
            (20.0, 3.0, 4.0, 3.0)
        );
        assert!(ClipPath::parse("circle(40%)").is_none());
        assert!(ClipPath::parse("inset(10% round 5px)").is_none());
        assert!(ClipPath::parse("inset(auto)").is_none());
    }

    #[test]
    fn clip_path_is_non_inherited_and_css_wide_keywords_resolve() {
        let dom = dom::parse(
            "<style>html{font-size:10px}div{clip-path:inset(1rem 10%)}span{clip-path:inherit}</style><div><span>x</span><b>y</b></div>",
        );
        let styles = StyleSet::from_dom(&dom, &[], 800.0);
        let parent = dom.elements_named("div").next().unwrap();
        let inherited = dom.elements_named("span").next().unwrap();
        let initial = dom.elements_named("b").next().unwrap();
        let expected = ClipPath::parse("inset(10px 10%)").unwrap();
        assert_eq!(styles.get(&parent).clip_path, expected);
        assert_eq!(styles.get(&inherited).clip_path, expected);
        assert_eq!(styles.get(&initial).clip_path, ClipPath::None);
    }
}
