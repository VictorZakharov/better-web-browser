//! Containing-block heights that cannot change a block's layout.
//
// A block receives its containing block's height so that its own percentage heights, and the
// insets of absolutely positioned children laid out against it, can resolve. Its in-flow children
// resolve theirs against the block's own specified height instead. A stretched grid item or a
// fixed-height scroller passes its current height down, so a block below one that grows (a column
// that gains a paragraph) is laid out with a new basis on every pass although nothing it contains
// reads it. Keying retained and measured layouts on that basis then misses every one of them, so
// the basis is dropped when nothing reads it, as Chrome's layout cache does for boxes without
// percentage-height dependents.
// https://drafts.csswg.org/css2/#the-height-property
// https://drafts.csswg.org/css2/#containing-block-details
use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    /// `basis` as `node`'s layout depends on it: `None` when no part of the layout reads it.
    pub(in crate::engine::layout) fn read_height_basis(
        &self,
        node: &NodeRef,
        basis: Option<f32>,
    ) -> Option<f32> {
        basis.filter(|_| !self.ignores_height_basis(node))
    }

    fn ignores_height_basis(&self, node: &NodeRef) -> bool {
        let style = self.styles.get(node);
        style.display == Display::Block
            && style.position == Position::Static
            && !style.height.has_percentage()
            && !style.min_height.has_percentage()
            && !style.max_height.has_percentage()
            && !node.is_popover_open()
            && !matches!(
                node.tag_name(),
                Some(
                    "html"
                        | "body"
                        | "img"
                        | "image"
                        | "video"
                        | "svg"
                        | "iframe"
                        | "canvas"
                        | "input"
                        | "textarea"
                        | "select"
                        | "button"
                        | "object"
                        | "embed"
                )
            )
            && self
                .box_children(node)
                .iter()
                .all(|child| self.styles.get(child).position != Position::Absolute)
    }
}

#[cfg(test)]
mod tests;
