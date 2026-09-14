//! Capture fragment geometry during the same traversal that positions paint atoms.
use super::*;
use fragments::{TextFragment, union};
use std::sync::Arc;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(in crate::engine::layout) fn record_atom_fragments(
        &mut self,
        measured: &MeasuredAtom<'_>,
        x: f32,
        y: f32,
        height: f32,
        line: u64,
    ) {
        if !self.retain_fragments {
            return;
        }
        match measured.atom {
            InlineAtom::Text {
                text: original,
                font,
                source_units,
                source_node,
                ..
            } => {
                let text = measured.text.unwrap_or_default();
                if text.is_empty() {
                    return;
                }
                let geometry = self.measurer.text_geometry(text, font);
                let text_y = y
                    + (height - measured.height).max(0.0) / 2.0
                    + (measured.height - measured.content_height) / 2.0;
                let skipped = original[..original.len() - text.len()]
                    .encode_utf16()
                    .count();
                let mut owners = Vec::new();
                for cluster in geometry.clusters.iter() {
                    let start = skipped + cluster.start as usize;
                    let end = skipped + cluster.end as usize;
                    let Some(units) = source_units.get(start..end) else {
                        continue;
                    };
                    let Some(unit) = units.first() else {
                        continue;
                    };
                    let Some(id) = unit.node else {
                        continue;
                    };
                    let mut retained = cluster.clone();
                    retained.start = units.iter().map(|u| u.start).min().unwrap();
                    retained.end = units.iter().map(|u| u.end).max().unwrap();
                    retained.rect.x += x;
                    retained.rect.y += text_y;
                    let fragments = Arc::make_mut(&mut self.output.fragments)
                        .text
                        .entry(id)
                        .or_default();
                    if fragments.last().is_none_or(|f| f.line != line) {
                        fragments.push(TextFragment {
                            line,
                            clusters: Vec::new(),
                        });
                    }
                    fragments
                        .last_mut()
                        .unwrap()
                        .clusters
                        .push(retained.clone());
                    if let Some((_, rect)) = owners.iter_mut().find(|(owner, _)| *owner == id) {
                        *rect = union(*rect, retained.rect);
                    } else {
                        owners.push((id, retained.rect));
                    }
                }
                // Generated text has no Range source, but still contributes to its inline box.
                if owners.is_empty()
                    && let Some(id) = source_node
                {
                    owners.push((
                        *id,
                        RectF {
                            x,
                            y: text_y,
                            width: measured.width,
                            height: geometry.bounds.height,
                        },
                    ));
                }
                for (id, rect) in owners {
                    self.record_inline_ancestors(id, rect, y, height, line);
                }
            }
            InlineAtom::BlockBox { node, .. } => self.record_inline_ancestors(
                node.id(),
                RectF {
                    x,
                    y,
                    width: measured.width,
                    height,
                },
                y,
                height,
                line,
            ),
            InlineAtom::Image { node_id, .. } => self.record_inline_ancestors(
                *node_id,
                RectF {
                    x,
                    y,
                    width: measured.width,
                    height,
                },
                y,
                height,
                line,
            ),
            InlineAtom::InlineBox {
                node_id: Some(id),
                children,
                ..
            } => {
                let rect = RectF {
                    x,
                    y,
                    width: measured.width,
                    height,
                };
                self.record_inline_ancestors(*id, rect, y, height, line);
                if children.is_empty() {
                    self.record_inline_box(*id, rect, y, height, line);
                }
            }
            _ => {}
        }
    }

    fn record_inline_ancestors(&mut self, id: NodeId, rect: RectF, y: f32, height: f32, line: u64) {
        let Some(node) = self.styles.node(id) else {
            return;
        };
        let mut parent = self.styles.parent(&node);
        while let Some(node) = parent {
            if self.styles.get(&node).display != Display::Inline {
                break;
            }
            self.record_inline_box(node.id(), rect, y, height, line);
            parent = self.styles.parent(&node);
        }
    }

    fn record_inline_box(&mut self, id: NodeId, rect: RectF, y: f32, height: f32, line: u64) {
        let Some(node) = self.styles.node(id) else {
            return;
        };
        let style = self.styles.get(&node);
        let font = FontSpec::from_style(style);
        // Inline boxes use their own font's ascent/descent, not a tall child's union.
        let geometry = self.measurer.text_geometry(" ", &font);
        let band = geometry.bounds;
        let measured_height = geometry.layout_height;
        let box_y = y
            + (height - style.line_height).max(0.0) / 2.0
            + (style.line_height - measured_height) / 2.0
            + band.y;
        Arc::make_mut(&mut self.output.fragments).push_inline(
            id,
            line,
            RectF {
                x: rect.x,
                y: box_y,
                width: rect.width,
                height: band.height,
            },
        );
    }
}
