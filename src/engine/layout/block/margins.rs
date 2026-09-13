//! Adjoining vertical margin sets retain positive and negative extrema.
//! https://www.w3.org/TR/CSS22/box.html#collapsing-margins
use super::super::*;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Default)]
pub(super) struct MarginStrut {
    positive: f32,
    negative: f32,
}

impl From<f32> for MarginStrut {
    fn from(value: f32) -> Self {
        Self::default().with(value)
    }
}

impl MarginStrut {
    pub(super) fn merge(self, other: Self) -> Self {
        self.with(other.positive).with(other.negative)
    }
    pub(super) fn with(self, value: f32) -> Self {
        Self {
            positive: self.positive.max(value),
            negative: self.negative.min(value),
        }
    }
    pub(super) fn size(self) -> f32 {
        self.positive + self.negative
    }
}

#[derive(Clone, Copy, Default)]
pub(in crate::engine::layout) struct MarginProfile {
    pub(super) top: MarginStrut,
    pub(super) bottom: MarginStrut,
    pub(super) through: bool,
    pub(super) absorb_start: bool,
    pub(super) absorb_end: bool,
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn block_establishes_context(&self, node: &NodeRef) -> bool {
        super::floats::establishes_context(self.styles.get(node))
            || matches!(node.tag_name(), Some("button" | "body" | "html"))
            || Node::composed_parent(node).is_some_and(|parent| {
                matches!(
                    self.styles.get(&parent).display,
                    Display::Flex | Display::InlineFlex | Display::Grid
                )
            })
    }

    pub(super) fn block_margin_profile(&self, node: &NodeRef, width: f32) -> MarginProfile {
        let key = (node.id(), width.to_bits());
        if let Some(profile) = self.margin_profiles.borrow().get(&key) {
            return *profile;
        }
        let profile = self.resolve_block_margin_profile(node, width);
        self.margin_profiles.borrow_mut().insert(key, profile);
        profile
    }

    fn resolve_block_margin_profile(&self, node: &NodeRef, width: f32) -> MarginProfile {
        let style = self.styles.get(node);
        let margins = style.margin.resolve(width, style.font_size);
        let mut profile = MarginProfile {
            top: margins.top.into(),
            bottom: margins.bottom.into(),
            ..Default::default()
        };
        if self.block_establishes_context(node)
            || style.display != Display::Block
            || input_control_data(node).is_some()
            || matches!(
                node.tag_name(),
                Some("img" | "image" | "video" | "svg" | "canvas" | "iframe" | "object")
            )
            || (node.tag_name() == Some("li") && style.list_style_type != ListStyleType::None)
        {
            return profile;
        }
        let padding = style.padding.resolve(width, style.font_size);
        let border = style.border_width.resolve(width, style.font_size);
        let insets = padding.horizontal() + border.horizontal();
        let child_width = (super::sizing::resolve_used_border_box_width(
            style,
            width,
            insets,
            margins,
            (width - margins.horizontal()).max(0.0),
            None,
        ) - insets)
            .max(0.0);
        let minimum_zero = matches!(style.min_height, Length::Auto | Length::Px(0.0));
        let mut start_open = padding.top + border.top == 0.0;
        let mut empty = true;
        let mut tail = MarginStrut::default();
        let mut end_block = false;
        for child in self.block_formatting_children(node) {
            let child_style = self.styles.get(&child);
            if child_style.display == Display::None
                || child_style.float != Float::None
                || matches!(child_style.position, Position::Absolute | Position::Fixed)
            {
                continue;
            }
            if child.element().is_none()
                && style.white_space != WhiteSpace::Pre
                && child.text_content().trim().is_empty()
            {
                continue;
            }
            if !is_block_level(child_style.display) || child.element().is_none() {
                start_open = false;
                empty = false;
                tail = Default::default();
                end_block = false;
                continue;
            }
            let child_profile = self.block_margin_profile(&child, child_width);
            if child_style.clear != crate::engine::css::Clear::None {
                start_open = false;
                empty = false;
            }
            if start_open {
                profile.top = profile.top.merge(child_profile.top);
                if child_profile.through {
                    profile.top = profile.top.merge(child_profile.bottom);
                }
                profile.absorb_start = true;
            }
            if !child_profile.through {
                empty = false;
                start_open = false;
                tail = child_profile.bottom;
            } else {
                tail = tail.merge(child_profile.top).merge(child_profile.bottom);
            }
            end_block = child_style.clear == crate::engine::css::Clear::None;
        }
        profile.absorb_end = end_block
            && style.height == Length::Auto
            && padding.bottom + border.bottom == 0.0
            && (minimum_zero || !empty);
        if profile.absorb_end {
            profile.bottom = profile.bottom.merge(tail);
        }
        profile.through = empty
            && minimum_zero
            && matches!(style.height, Length::Auto | Length::Px(0.0))
            && padding.vertical() + border.vertical() == 0.0;
        profile
    }
}
