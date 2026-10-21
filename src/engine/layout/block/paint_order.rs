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
        let effect_context = style.opacity < 1.0
            || style.has_transform()
            || !style.clip_path.clone().is_none()
            || !style.mask_image.0.is_empty();
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

/// Items and hit-test node identifiers in final paint order, as a linked run of arena entries.
/// Paint order nests one level per box, so contents are linked rather than copied: appending a
/// child's contents or wrapping them in a begin/end pair costs the same at every depth, and each
/// display item moves only into and out of the arena.
#[derive(Default)]
struct Contents {
    /// The first and last entry, or `None` when empty.
    ends: Option<(usize, usize)>,
}

enum Entry {
    Item(DisplayItem),
    Node(NodeId),
}

/// Each entry and the index of the entry that follows it.
#[derive(Default)]
struct Arena {
    entries: Vec<(Option<Entry>, Option<usize>)>,
}

impl Arena {
    fn single(&mut self, entry: Entry) -> Contents {
        let index = self.entries.len();
        self.entries.push((Some(entry), None));
        Contents {
            ends: Some((index, index)),
        }
    }

    fn append(&mut self, contents: &mut Contents, other: Contents) {
        let Some((other_head, other_tail)) = other.ends else {
            return;
        };
        contents.ends = Some(match contents.ends {
            Some((head, tail)) => {
                self.entries[tail].1 = Some(other_head);
                (head, other_tail)
            }
            None => (other_head, other_tail),
        });
    }

    fn push(&mut self, contents: &mut Contents, entry: Entry) {
        let single = self.single(entry);
        self.append(contents, single);
    }

    /// Wraps `contents` in a begin/end item pair.
    fn wrap(&mut self, contents: &mut Contents, begin: DisplayItem, end: DisplayItem) {
        let mut wrapped = self.single(Entry::Item(begin));
        self.append(&mut wrapped, std::mem::take(contents));
        self.push(&mut wrapped, Entry::Item(end));
        *contents = wrapped;
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
fn parse(arena: &mut Arena, items: &mut std::vec::IntoIter<DisplayItem>) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    while let Some(item) = items.next() {
        match item {
            DisplayItem::PaintBoundary {
                entering: false, ..
            }
            | DisplayItem::EndClip { .. }
            | DisplayItem::EndOpacity { .. }
            | DisplayItem::EndTransform { .. }
            | DisplayItem::EndMask { .. }
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
                let children = parse(arena, items);
                if kind == DECORATION {
                    let mut content = flatten(arena, children);
                    if let Some(node_id) = node_id {
                        arena.push(&mut content, Entry::Node(node_id));
                    }
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
                        content: order_context(arena, local),
                    });
                    chunks.extend(escaping);
                }
            }
            DisplayItem::BeginClip { bounds } => {
                let mut children = parse(arena, items);
                if children.is_empty() {
                    children.push(Chunk::new(Phase::Content, Contents::default()));
                }
                for mut child in children {
                    arena.wrap(
                        &mut child.content,
                        DisplayItem::BeginClip { bounds },
                        DisplayItem::EndClip { bounds },
                    );
                    chunks.push(child);
                }
            }
            DisplayItem::BeginCssMask { bounds, layers } => {
                let children = parse(arena, items);
                let mut content = order_context(arena, children);
                arena.wrap(
                    &mut content,
                    DisplayItem::BeginCssMask { bounds, layers },
                    DisplayItem::EndMask { bounds },
                );
                chunks.push(Chunk::new(Phase::Content, content));
            }
            DisplayItem::BeginOpacity { bounds, opacity } => {
                let children = parse(arena, items);
                let mut content = order_context(arena, children);
                arena.wrap(
                    &mut content,
                    DisplayItem::BeginOpacity { bounds, opacity },
                    DisplayItem::EndOpacity { bounds },
                );
                chunks.push(Chunk::new(Phase::Content, content));
            }
            DisplayItem::BeginTransform { bounds, matrix } => {
                let children = parse(arena, items);
                let mut content = order_context(arena, children);
                arena.wrap(
                    &mut content,
                    DisplayItem::BeginTransform { bounds, matrix },
                    DisplayItem::EndTransform { bounds },
                );
                chunks.push(Chunk::new(Phase::Content, content));
            }
            DisplayItem::NodeBoundary {
                node_id,
                entering: true,
            } => {
                let children = parse(arena, items);
                let mut content = order_context(arena, children);
                arena.wrap(
                    &mut content,
                    DisplayItem::NodeBoundary {
                        node_id,
                        entering: true,
                    },
                    DisplayItem::NodeBoundary {
                        node_id,
                        entering: false,
                    },
                );
                chunks.push(Chunk::new(Phase::Content, content));
            }
            item => {
                // Replaced controls paint without a block decoration marker.
                // Preserve their owner in the same ordered stream as box hits.
                let control_id = match &item {
                    DisplayItem::Control(control) => Some(control.node_id),
                    _ => None,
                };
                if !chunks
                    .last()
                    .is_some_and(|last| last.phase == Phase::Content && !last.escapes)
                {
                    chunks.push(Chunk::new(Phase::Content, Contents::default()));
                }
                let content = &mut chunks.last_mut().expect("a content chunk").content;
                arena.push(content, Entry::Item(item));
                if let Some(control_id) = control_id {
                    arena.push(content, Entry::Node(control_id));
                }
            }
        }
    }
    chunks
}

fn flatten(arena: &mut Arena, chunks: Vec<Chunk>) -> Contents {
    let mut result = Contents::default();
    for chunk in chunks {
        arena.append(&mut result, chunk.content);
    }
    result
}
fn order_context(arena: &mut Arena, mut chunks: Vec<Chunk>) -> Contents {
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
    let rest = flatten(arena, chunks);
    arena.append(&mut own, rest);
    own
}
pub(in crate::engine::layout) fn finalize(output: &mut LayoutOutput) {
    let items = std::mem::take(&mut output.items);
    let mut arena = Arena {
        entries: Vec::with_capacity(items.len() + items.len() / 4),
    };
    let chunks = parse(&mut arena, &mut items.into_iter());
    let content = flatten(&mut arena, chunks);
    let mut items = Vec::with_capacity(arena.entries.len());
    // Hit-test geometry must follow the same final order as visible paint, including
    // backgrounds without text (popup padding is interactive too).
    let mut nodes = Vec::new();
    let mut next = content.ends.map(|(head, _)| head);
    while let Some(index) = next {
        let (entry, following) = &mut arena.entries[index];
        match entry.take().expect("each paint-order entry is linked once") {
            Entry::Item(item) => items.push(item),
            Entry::Node(node) => nodes.push(node),
        }
        next = *following;
    }
    output.items = items;
    output.node_paint_order = nodes;
}

#[cfg(test)]
mod stacking_tests;
#[cfg(test)]
mod tests;
