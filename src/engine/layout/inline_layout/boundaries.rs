//! CSS Text §5.1: atomic inline edges create soft wraps in the surrounding context.
//! https://www.w3.org/TR/css-text-3/#line-break-details
use super::*;
use std::collections::HashSet;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(in crate::engine::layout) fn inline_break_before(
        &self,
        atoms: &[InlineAtom],
        index: usize,
    ) -> bool {
        let right = &atoms[index];
        if let InlineAtom::Text { text, no_wrap, .. } = right
            && text.starts_with([' ', '\t', '\n', '\r', '\u{c}'])
        {
            return !no_wrap;
        }
        let Some(left) = index.checked_sub(1).map(|i| &atoms[i]) else {
            return false;
        };
        if !atomic(left) && !atomic(right) {
            return false;
        }
        if boundary_joiner(left, false) || boundary_joiner(right, true) {
            return false;
        }
        let left = self.atom_node(left);
        let right = self.atom_node(right);
        match (left, right) {
            (Some(left), Some(right)) => {
                let ancestors: HashSet<_> =
                    std::iter::successors(Some(left), Node::composed_parent)
                        .map(|n| n.id())
                        .collect();
                std::iter::successors(Some(right), Node::composed_parent)
                    .find(|n| ancestors.contains(&n.id()))
                    .is_none_or(|n| self.styles.get(&n).white_space == WhiteSpace::Normal)
            }
            (Some(node), None) | (None, Some(node)) => Node::composed_parent(&node)
                .is_none_or(|n| self.styles.get(&n).white_space == WhiteSpace::Normal),
            _ => true,
        }
    }

    fn atom_node(&self, atom: &InlineAtom) -> Option<NodeRef> {
        let id = match atom {
            InlineAtom::BlockBox { node, .. } => return Some(node.clone()),
            InlineAtom::Text {
                source_node,
                node_id,
                ..
            } => source_node.or(*node_id),
            InlineAtom::Image { node_id, .. } => Some(*node_id),
            InlineAtom::InlineBox { node_id, .. } | InlineAtom::Placeholder { node_id, .. } => {
                *node_id
            }
            InlineAtom::Control { spec, .. } => Some(spec.node_id),
            InlineAtom::Break => None,
        }?;
        self.styles.node(id)
    }
}

fn atomic(atom: &InlineAtom) -> bool {
    matches!(
        atom,
        InlineAtom::BlockBox { .. }
            | InlineAtom::Image { .. }
            | InlineAtom::Control { .. }
            | InlineAtom::Placeholder { .. }
    )
}

fn boundary_joiner(atom: &InlineAtom, first: bool) -> bool {
    let InlineAtom::Text { text, .. } = atom else {
        return false;
    };
    let character = if first {
        text.chars().next()
    } else {
        text.chars().next_back()
    };
    // Explicit word/joining controls, narrow/figure spaces and nonbreaking hyphen.
    // NBSP is deliberately excluded: CSS's atomic-inline compatibility exception.
    matches!(
        character,
        Some(
            '\u{2060}'
                | '\u{feff}'
                | '\u{200d}'
                | '\u{202f}'
                | '\u{2007}'
                | '\u{2011}'
                | '\u{034f}'
        )
    )
}
