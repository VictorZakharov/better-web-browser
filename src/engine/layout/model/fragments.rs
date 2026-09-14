//! Renderer-local CSSOM fragment geometry. Source ranges use DOM UTF-16 offsets.
use super::*;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub struct TextCluster {
    pub start: u32,
    pub end: u32,
    pub rect: RectF,
    pub rtl: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextGeometry {
    pub layout_height: f32,
    /// Font ascent/descent band relative to the shaped run's layout origin.
    pub bounds: RectF,
    pub clusters: Arc<[TextCluster]>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SourceUnit {
    pub node: Option<NodeId>,
    pub start: u32,
    pub end: u32,
}

pub(crate) type PendingSpace = Option<SourceUnit>;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InlineFragment {
    pub line: u64,
    pub rect: RectF,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TextFragment {
    pub line: u64,
    pub clusters: Vec<TextCluster>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FragmentGeometry {
    pub(crate) elements: HashMap<NodeId, Vec<InlineFragment>>,
    pub(crate) text: HashMap<NodeId, Vec<TextFragment>>,
    pub(crate) next_line: u64,
}

impl FragmentGeometry {
    pub(crate) fn element(&self, id: NodeId) -> Option<Vec<RectF>> {
        self.elements
            .get(&id)
            .map(|fragments| fragments.iter().map(|f| f.rect).collect())
    }

    pub(crate) fn push_inline(&mut self, id: NodeId, line: u64, rect: RectF) {
        let fragments = self.elements.entry(id).or_default();
        if let Some(last) = fragments.last_mut().filter(|last| last.line == line) {
            last.rect = union(last.rect, rect);
        } else {
            fragments.push(InlineFragment { line, rect });
        }
    }

    pub(crate) fn translate(&mut self, id: NodeId, dx: f32, dy: f32) {
        if let Some(fragments) = self.elements.get_mut(&id) {
            for fragment in fragments {
                fragment.rect.x += dx;
                fragment.rect.y += dy;
            }
        }
        if let Some(fragments) = self.text.get_mut(&id) {
            for fragment in fragments {
                for cluster in &mut fragment.clusters {
                    cluster.rect.x += dx;
                    cluster.rect.y += dy;
                }
            }
        }
    }

    pub(crate) fn remove(&mut self, id: NodeId) {
        self.elements.remove(&id);
        self.text.remove(&id);
    }

    pub(crate) fn range_text(&self, id: NodeId, start: u32, end: u32) -> Vec<RectF> {
        let Some(fragments) = self.text.get(&id) else {
            return Vec::new();
        };
        let mut result: Vec<(u64, RectF)> = Vec::new();
        for fragment in fragments {
            for cluster in &fragment.clusters {
                let rect = if start == end {
                    if start < cluster.start || start > cluster.end {
                        continue;
                    }
                    let mut rect = cluster.rect;
                    if start == cluster.start || start == cluster.end {
                        if (start == cluster.end) != cluster.rtl {
                            rect.x = rect.right();
                        }
                        rect.width = 0.0;
                    }
                    rect
                } else {
                    if start >= cluster.end || end <= cluster.start {
                        continue;
                    }
                    cluster.rect
                };
                if let Some((line, last)) =
                    result.last_mut().filter(|(line, _)| *line == fragment.line)
                {
                    let _ = line;
                    *last = union(*last, rect);
                } else {
                    result.push((fragment.line, rect));
                }
            }
        }
        result.into_iter().map(|(_, rect)| rect).collect()
    }
}

pub(crate) fn union(a: RectF, b: RectF) -> RectF {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    RectF {
        x,
        y,
        width: a.right().max(b.right()) - x,
        height: a.bottom().max(b.bottom()) - y,
    }
}

/// Simple embedders without a shaper still measure whole prefixes, never raster bounds.
/// Production renderer overrides this with its retained OpenType shaping clusters.
pub(super) fn measured_geometry<M: TextMeasurer + ?Sized>(
    measurer: &mut M,
    text: &str,
    font: &FontSpec,
) -> TextGeometry {
    use unicode_segmentation::UnicodeSegmentation;
    let (width, height) = measurer.measure(text, font);
    let mut clusters = Vec::new();
    let mut offset = 0;
    let mut left = 0.0;
    for (byte, grapheme) in text.grapheme_indices(true) {
        let right = measurer.measure(&text[..byte + grapheme.len()], font).0;
        let end = offset + grapheme.encode_utf16().count() as u32;
        clusters.push(TextCluster {
            start: offset,
            end,
            rect: RectF {
                x: left,
                y: 0.0,
                width: right - left,
                height,
            },
            rtl: false,
        });
        offset = end;
        left = right;
    }
    TextGeometry {
        layout_height: height,
        bounds: RectF {
            x: 0.0,
            y: 0.0,
            width,
            height,
        },
        clusters: clusters.into(),
    }
}
