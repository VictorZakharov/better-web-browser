use super::*;
use crate::engine::css::{Display, Position};

impl HostState {
    /// Whether the box belongs to a viewport-fixed subtree rather than document scrolling.
    pub(in crate::engine::script) fn is_viewport_fixed(&mut self, node: &NodeRef) -> bool {
        let (version, mut styles) = self.take_offset_parent_styles();
        let ancestors = std::iter::successors(Some(node.clone()), Node::composed_parent)
            .filter(|node| node.element().is_some())
            .collect::<Vec<_>>();
        let mut containing_block = false;
        let mut viewport_fixed = false;
        for ancestor in ancestors.iter().rev() {
            let Some(style) = styles.computed_style_for_node(ancestor) else {
                continue;
            };
            if matches!(style.display, Display::None | Display::Contents) {
                continue;
            }
            // Only fixed boxes without an ancestor fixed-position containing block attach to
            // the viewport. Their descendants stay attached even through another transform.
            // https://www.w3.org/TR/css-position-3/#fixed-cb
            if style.position == Position::Fixed && !containing_block {
                viewport_fixed = true;
                break;
            }
            containing_block |= style.establishes_fixed_position_containing_block();
        }
        self.offset_parent_styles = Some((version, styles));
        viewport_fixed
    }
}
