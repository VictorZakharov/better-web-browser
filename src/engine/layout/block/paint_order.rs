//! CSS 2.2 Appendix E paint phases, with positioned descendants escaping non-contexts.
//! Geometry stays in flow order; local markers survive transforms and flex/grid placement.
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
        let effect_context = style.opacity < 1.0 || !style.transform.is_none();
        let positioned = style.position != Position::Static;
        let root = matches!(node.tag_name(), Some("body" | "html"));
        let isolates = root
            || effect_context
            || matches!(style.position, Position::Fixed | Position::Sticky)
            || (positioned && style.z_index.is_some());
        let level = if positioned {
            style.z_index.unwrap_or(0)
        } else {
            0
        };
        let kind = if positioned || effect_context {
            if level < 0 {
                NEGATIVE_GROUP
            } else {
                POSITIONED_GROUP
            }
        } else if style.float != Float::None {
            FLOAT_GROUP
        } else if root
            || matches!(
                style.display,
                Display::InlineBlock | Display::InlineFlex | Display::InlineTable
            )
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
                level,
                isolates,
                node_id: None,
            },
        );
        self.output.items.push(DisplayItem::PaintBoundary {
            kind,
            entering: false,
            level,
            isolates,
            node_id: None,
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

#[derive(Default)]
struct Contents {
    items: Vec<DisplayItem>,
    nodes: Vec<NodeId>,
}
impl Contents {
    fn append(&mut self, other: Self) {
        self.items.extend(other.items);
        self.nodes.extend(other.nodes);
    }
}
struct Chunk {
    phase: Phase,
    level: i32,
    escapes: bool,
    content: Contents,
}
impl Chunk {
    fn new(phase: Phase, content: Contents) -> Self {
        Self {
            phase,
            level: 0,
            escapes: false,
            content,
        }
    }
}

// Clips do not establish stacking contexts. Each escaped paint chunk keeps a balanced
// clip pair, so moving it in z order cannot remove an ancestor's clipping constraint.
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
                level,
                isolates,
                node_id,
            } => {
                let children = parse(items);
                if kind == DECORATION {
                    let mut content = flatten(children);
                    content.nodes.extend(node_id);
                    chunks.push(Chunk::new(Phase::Background, content));
                } else {
                    let phase = match kind {
                        FLOAT_GROUP => Phase::Float,
                        NEGATIVE_GROUP => Phase::Negative,
                        POSITIONED_GROUP => Phase::Positioned,
                        _ => Phase::Content,
                    };
                    // Inline-blocks, floats and z-index:auto are atomic for their normal
                    // contents, not stacking contexts for their positioned descendants.
                    let (escaping, local): (Vec<_>, Vec<_>) = children
                        .into_iter()
                        .partition(|child| !isolates && child.escapes);
                    chunks.push(Chunk {
                        phase,
                        level,
                        escapes: isolates || matches!(phase, Phase::Negative | Phase::Positioned),
                        content: order_context(local),
                    });
                    chunks.extend(escaping);
                }
            }
            DisplayItem::BeginClip { bounds } => {
                let mut children = parse(items);
                if children.is_empty() {
                    children.push(Chunk::new(Phase::Content, Contents::default()));
                }
                for mut child in children {
                    child
                        .content
                        .items
                        .insert(0, DisplayItem::BeginClip { bounds });
                    child.content.items.push(DisplayItem::EndClip { bounds });
                    chunks.push(child);
                }
            }
            DisplayItem::BeginOpacity { bounds, opacity } => {
                let mut content = order_context(parse(items));
                content
                    .items
                    .insert(0, DisplayItem::BeginOpacity { bounds, opacity });
                content.items.push(DisplayItem::EndOpacity { bounds });
                chunks.push(Chunk::new(Phase::Content, content));
            }
            DisplayItem::NodeBoundary {
                node_id,
                entering: true,
            } => {
                let mut content = order_context(parse(items));
                content.items.insert(
                    0,
                    DisplayItem::NodeBoundary {
                        node_id,
                        entering: true,
                    },
                );
                content.items.push(DisplayItem::NodeBoundary {
                    node_id,
                    entering: false,
                });
                chunks.push(Chunk::new(Phase::Content, content));
            }
            item => {
                if let Some(last) = chunks
                    .last_mut()
                    .filter(|last| last.phase == Phase::Content && !last.escapes)
                {
                    last.content.items.push(item);
                } else {
                    chunks.push(Chunk::new(
                        Phase::Content,
                        Contents {
                            items: vec![item],
                            nodes: Vec::new(),
                        },
                    ));
                }
            }
        }
    }
    chunks
}

fn flatten(chunks: Vec<Chunk>) -> Contents {
    let mut result = Contents::default();
    for chunk in chunks {
        result.append(chunk.content);
    }
    result
}
fn order_context(mut chunks: Vec<Chunk>) -> Contents {
    // The context's own background precedes its negative descendants; other backgrounds
    // follow them. Stable sorting preserves tree order for equal stack levels.
    let mut own = if chunks
        .first()
        .is_some_and(|chunk| chunk.phase == Phase::Background)
    {
        chunks.remove(0).content
    } else {
        Contents::default()
    };
    chunks.sort_by_key(|chunk| (chunk.phase, chunk.level));
    own.append(flatten(chunks));
    own
}
pub(in crate::engine::layout) fn finalize(output: &mut LayoutOutput) {
    let content = flatten(parse(&mut std::mem::take(&mut output.items).into_iter()));
    output.items = content.items;
    // Hit-test geometry must follow the same final order as visible paint, including
    // backgrounds without text (popup padding is interactive too).
    output.node_paint_order = content.nodes;
}

#[cfg(test)]
mod stacking_tests;
#[cfg(test)]
mod tests;
