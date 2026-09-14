//! Cyclic percentage constraints compress an image's minimum, not its preferred width.
//! https://www.w3.org/TR/css-sizing-3/#cyclic-percentage-contribution
use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(in crate::engine::layout) fn replaced_intrinsic_widths(
        &self,
        atom: &InlineAtom,
        basis: Option<f32>,
    ) -> Option<(f32, f32)> {
        if basis.is_some() {
            return None;
        }
        let (id, minimum) = match atom {
            InlineAtom::Image { node_id, width, .. } => (*node_id, *width),
            InlineAtom::Placeholder {
                node_id: Some(id),
                width,
                ..
            } => (*id, *width),
            _ => return None,
        };
        let node = self.styles.node(id)?;
        if !matches!(node.tag_name(), Some("img" | "image" | "video")) {
            return None;
        }
        let mut style = self.styles.get(&node).clone();
        if !cyclic(style.width) && !cyclic(style.max_width) {
            return None;
        }
        if cyclic(style.width) {
            style.width = Length::Auto;
        }
        if cyclic(style.max_width) {
            style.max_width = Length::Auto;
        }
        let mut atoms = Vec::new();
        self.collect_image(
            &node,
            &style,
            None,
            &mut atoms,
            InlineContainingBlock {
                width: 0.0,
                height: None,
            },
        );
        let preferred = atoms.first().and_then(|atom| match atom {
            InlineAtom::Image { width, .. } | InlineAtom::Placeholder { width, .. } => Some(*width),
            _ => None,
        })?;
        Some((minimum, preferred.max(minimum)))
    }
}

fn cyclic(length: Length) -> bool {
    matches!(length, Length::Percent(_))
        || matches!(length, Length::Calc { percent, .. } if percent != 0.0)
}
