//! Buttons use the same authored descendant layout in inline and block formatting contexts.
use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(in crate::engine::layout) fn collect_button(
        &self,
        node: &NodeRef,
        _style: &ComputedStyle,
        output: &mut Vec<InlineAtom>,
        containing_block: InlineContainingBlock,
    ) {
        output.push(InlineAtom::BlockBox {
            node: node.clone(),
            height_basis: containing_block.height,
        });
    }
}
