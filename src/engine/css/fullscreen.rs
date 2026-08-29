//! Fullscreen top-layer user-agent style policy.

use super::*;

pub(super) fn apply_fullscreen_ua_style(
    node: &NodeRef,
    style: &mut ComputedStyle,
    viewport_width: f32,
    viewport_height: f32,
) {
    if !node.is_fullscreen() {
        return;
    }
    // These important UA declarations apply after the author cascade without mutating author DOM.
    // https://fullscreen.spec.whatwg.org/#user-agent-level-style-sheet-defaults
    style.position = Position::Fixed;
    style.box_sizing = BoxSizing::BorderBox;
    style.margin = Edges::ZERO;
    style.top = Length::Px(0.0);
    style.right = Length::Px(0.0);
    style.bottom = Length::Px(0.0);
    style.left = Length::Px(0.0);
    style.width = Length::Px(viewport_width);
    style.height = Length::Px(viewport_height);
    style.min_width = Length::Px(0.0);
    style.min_height = Length::Px(0.0);
    style.max_width = Length::Auto;
    style.max_height = Length::Auto;
}
