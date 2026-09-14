//! CSS 2.2 Appendix E: normal block decoration precedes floats, then inline content.
//! Geometry stays in flow order. Renderer-local markers retain ownership while layout moves
//! flex/grid/positioned subtrees, and are consumed only after all geometry is final.
//! https://www.w3.org/TR/CSS22/zindex.html#painting-order
use super::super::*;

pub(super) const DECORATION: u8 = 0;
const FLOAT_GROUP: u8 = 1;
const ATOMIC_GROUP: u8 = 2;
const NEGATIVE_GROUP: u8 = 3;
const POSITIONED_GROUP: u8 = 4;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn wrap_block_paint(&mut self, node: &NodeRef, style: &ComputedStyle, start: usize) {
        if !self.emit_paint {
            return;
        }
        let kind = if style.position != Position::Static {
            if style.z_index.is_some_and(|level| level < 0) {
                NEGATIVE_GROUP
            } else {
                POSITIONED_GROUP
            }
        } else if style.float != Float::None {
            FLOAT_GROUP
        } else if style.opacity < 1.0
            || !style.transform.is_none()
            || matches!(
                style.display,
                Display::InlineBlock | Display::InlineFlex | Display::InlineTable
            )
            || matches!(node.tag_name(), Some("body" | "html"))
        {
            ATOMIC_GROUP
        } else {
            return;
        };
        self.output.items.insert(
            start,
            DisplayItem::PaintBoundary {
                kind,
                entering: true,
            },
        );
        self.output.items.push(DisplayItem::PaintBoundary {
            kind,
            entering: false,
        });
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Phase {
    Negative,
    Background,
    Float,
    Content,
    Positioned,
}
struct Chunk {
    phase: Phase,
    items: Vec<DisplayItem>,
}

// Clips do not establish stacking contexts. Splitting a clip into balanced groups in each
// phase preserves clipping when a descendant's background and text surround a sibling float.
fn parse(items: &mut std::vec::IntoIter<DisplayItem>) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    while let Some(item) = items.next() {
        match item {
            DisplayItem::PaintBoundary {
                entering: false, ..
            }
            | DisplayItem::EndClip { .. }
            | DisplayItem::EndOpacity { .. }
            | DisplayItem::NodeBoundary {
                entering: false, ..
            } => break,
            DisplayItem::PaintBoundary {
                kind,
                entering: true,
            } => {
                let children = parse(items);
                let (phase, content) = if kind == DECORATION {
                    (Phase::Background, flatten(children))
                } else {
                    let phase = match kind {
                        FLOAT_GROUP => Phase::Float,
                        NEGATIVE_GROUP => Phase::Negative,
                        POSITIONED_GROUP => Phase::Positioned,
                        _ => Phase::Content,
                    };
                    (phase, order_context(children))
                };
                chunks.push(Chunk {
                    phase,
                    items: content,
                });
            }
            DisplayItem::BeginClip { bounds } => {
                let mut children = parse(items);
                if children.is_empty() {
                    children.push(Chunk {
                        phase: Phase::Content,
                        items: Vec::new(),
                    });
                }
                for mut child in children {
                    child.items.insert(0, DisplayItem::BeginClip { bounds });
                    child.items.push(DisplayItem::EndClip { bounds });
                    chunks.push(child);
                }
            }
            DisplayItem::BeginOpacity { bounds, opacity } => {
                let mut content = order_context(parse(items));
                content.insert(0, DisplayItem::BeginOpacity { bounds, opacity });
                content.push(DisplayItem::EndOpacity { bounds });
                chunks.push(Chunk {
                    phase: Phase::Content,
                    items: content,
                });
            }
            DisplayItem::NodeBoundary {
                node_id,
                entering: true,
            } => {
                let mut content = order_context(parse(items));
                content.insert(
                    0,
                    DisplayItem::NodeBoundary {
                        node_id,
                        entering: true,
                    },
                );
                content.push(DisplayItem::NodeBoundary {
                    node_id,
                    entering: false,
                });
                chunks.push(Chunk {
                    phase: Phase::Content,
                    items: content,
                });
            }
            item => {
                if let Some(last) = chunks
                    .last_mut()
                    .filter(|last| last.phase == Phase::Content)
                {
                    last.items.push(item);
                } else {
                    chunks.push(Chunk {
                        phase: Phase::Content,
                        items: vec![item],
                    });
                }
            }
        }
    }
    chunks
}
fn flatten(chunks: Vec<Chunk>) -> Vec<DisplayItem> {
    chunks.into_iter().flat_map(|chunk| chunk.items).collect()
}
fn order_context(mut chunks: Vec<Chunk>) -> Vec<DisplayItem> {
    // A context's own background is below its negative descendants; normal descendant
    // backgrounds are above them. Empty decoration markers preserve that distinction.
    let own = if chunks
        .first()
        .is_some_and(|chunk| chunk.phase == Phase::Background)
    {
        chunks.remove(0).items
    } else {
        Vec::new()
    };
    chunks.sort_by_key(|chunk| chunk.phase);
    own.into_iter().chain(flatten(chunks)).collect()
}
pub(in crate::engine::layout) fn finalize(items: &mut Vec<DisplayItem>) {
    *items = flatten(parse(&mut std::mem::take(items).into_iter()));
}

#[cfg(test)]
mod tests;
