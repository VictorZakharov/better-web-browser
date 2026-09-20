//! Grapheme-safe paint elision for CSS Overflow 3 and legacy line clamping.
//! https://drafts.csswg.org/css-overflow-3/#text-overflow
//! https://drafts.csswg.org/css-overflow-4/#legacy-compatibility
//!
//! A plan describes paint only: original advances, boxes and source fragments
//! remain intact. Nested inline wrappers share one marker budget; atomic boxes
//! are never shortened internally.
use super::*;
use unicode_segmentation::UnicodeSegmentation;

pub(super) const ELLIPSIS: &str = "…";

#[derive(Debug, Clone)]
pub(super) struct TruncationPolicy {
    pub single_line_ellipsis: bool,
    pub max_lines: Option<u32>,
    pub container_font: FontSpec,
    pub container_color: Color,
    pub container_visible: bool,
    pub container_line_height: f32,
}
impl TruncationPolicy {
    pub(super) fn for_style(style: &ComputedStyle) -> Self {
        Self {
            single_line_ellipsis: style.ellipsis_on_overflow(),
            max_lines: style.clamp_max_lines(),
            container_font: FontSpec::from_style(style),
            container_color: style.color,
            container_visible: style.visibility,
            container_line_height: style.line_height,
        }
    }
}

/// Fresh per formatting context; shared by that container's inline flushes.
#[derive(Debug, Clone, Default)]
pub(super) struct ClampState {
    pub lines_used: u32,
    pub finished: bool,
}
impl ClampState {
    pub(super) fn fresh(policy: &TruncationPolicy) -> Option<Self> {
        policy.max_lines.map(|_| Self::default())
    }
}

#[derive(Debug, Clone)]
pub(super) enum AtomKeep {
    Whole,
    TextPrefix {
        len: usize,
        width: f32,
    },
    /// Kept child indices; the original inline box geometry is unchanged.
    BoxPrefix {
        keeps: Vec<(usize, AtomKeep)>,
    },
}

#[derive(Debug, Clone)]
pub(super) struct TruncationMarker {
    pub width: f32,
    pub font: FontSpec,
    pub color: Color,
    pub link: Option<String>,
    pub node_id: Option<NodeId>,
    pub visible: bool,
    pub ref_height: f32,
    pub ref_content_height: f32,
}
#[derive(Debug, Clone)]
pub(super) struct RowPlan {
    pub keeps: Vec<(usize, AtomKeep)>,
    pub marker: TruncationMarker,
    pub width: f32,
}

/// Visual style belongs to the block, independently of the target's link/node.
fn marker_owner(atom: &InlineAtom, keep: &AtomKeep) -> (Option<String>, Option<NodeId>) {
    match (atom, keep) {
        (InlineAtom::Text { link, node_id, .. }, _) => (link.clone(), *node_id),
        (InlineAtom::InlineBox { children, .. }, AtomKeep::BoxPrefix { keeps, .. }) => {
            keeps.last().map_or((None, None), |(index, keep)| {
                marker_owner(&children[*index], keep)
            })
        }
        (InlineAtom::InlineBox { children, .. }, AtomKeep::Whole) => children
            .last()
            .map_or((None, None), |child| marker_owner(child, &AtomKeep::Whole)),
        _ => (None, None),
    }
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    /// Binary search only at extended-grapheme boundaries. Measurement does not
    /// allocate raster runs and retained source geometry is collected separately.
    fn fit_text_prefix(&mut self, text: &str, font: &FontSpec, budget: f32) -> (usize, f32) {
        if text.is_empty() {
            return (0, 0.0);
        }
        let boundaries: Vec<usize> = text
            .grapheme_indices(true)
            .map(|(index, _)| index)
            .chain(std::iter::once(text.len()))
            .collect();
        let (mut low, mut high) = (0, boundaries.len() - 1);
        while low + 1 < high {
            let mid = (low + high) / 2;
            if self.measurer.measure(&text[..boundaries[mid]], font).0 <= budget {
                low = mid;
            } else {
                high = mid;
            }
        }
        let len = text[..boundaries[low]].trim_end_matches([' ', '\t']).len();
        (len, self.measurer.measure(&text[..len], font).0)
    }

    pub(super) fn plan_truncated_row(
        &mut self,
        line: &[MeasuredAtom<'_>],
        available: f32,
        containing: f32,
        policy: &TruncationPolicy,
    ) -> Option<RowPlan> {
        // CSS Overflow 3: style and baseline-align the marker according to the
        // block, not the last text run (which can have a different font/color).
        let (reserve, content_height) = self.measurer.measure(ELLIPSIS, &policy.container_font);
        if reserve > available || reserve <= 0.0 {
            return None;
        }
        let (keeps, kept_width) = self.fit_row(line, available - reserve, containing);
        if kept_width <= 0.0 {
            return None;
        }
        let (link, node_id) = keeps.last().map_or((None, None), |(index, keep)| {
            marker_owner(line[*index].atom, keep)
        });
        Some(RowPlan {
            keeps,
            width: kept_width + reserve,
            marker: TruncationMarker {
                width: reserve,
                font: policy.container_font.clone(),
                color: policy.container_color,
                link,
                node_id,
                visible: policy.container_visible,
                ref_height: policy.container_line_height,
                ref_content_height: content_height,
            },
        })
    }

    /// Fit content into a budget from which the ONE line marker was already
    /// deducted. Recursion must not reserve another marker per inline wrapper.
    fn fit_row(
        &mut self,
        line: &[MeasuredAtom<'_>],
        budget: f32,
        containing: f32,
    ) -> (Vec<(usize, AtomKeep)>, f32) {
        let mut keeps = Vec::new();
        let mut used = 0.0;
        for (index, measured) in line.iter().enumerate() {
            let remaining = (budget - used).max(0.0);
            if measured.width <= remaining {
                keeps.push((index, AtomKeep::Whole));
                used += measured.width;
                continue;
            }
            match measured.atom {
                InlineAtom::Text { font, .. } => {
                    let (len, width) =
                        self.fit_text_prefix(measured.text.unwrap_or_default(), font, remaining);
                    if len > 0 {
                        keeps.push((index, AtomKeep::TextPrefix { len, width }));
                        used += width;
                    }
                }
                InlineAtom::InlineBox {
                    children, style, ..
                } if style.display == Display::Inline => {
                    let metrics =
                        self.measure_inline_box(measured.atom, children, style, containing);
                    let left = metrics.margin.left + metrics.border.left + metrics.padding.left;
                    let content_width = (metrics.border_box_width
                        - metrics.border.horizontal()
                        - metrics.padding.horizontal())
                    .max(0.0);
                    let indices: Vec<_> = children
                        .iter()
                        .enumerate()
                        .filter(|(_, child)| !matches!(child, InlineAtom::Break))
                        .collect();
                    let row: Vec<_> = indices
                        .iter()
                        .map(|(i, child)| self.measure_atom(child, *i == 0, content_width))
                        .collect();
                    let (child_keeps, width) =
                        self.fit_row(&row, (remaining - left).max(0.0), content_width);
                    if width > 0.0 && left + width <= remaining {
                        let child_keeps = child_keeps
                            .into_iter()
                            .map(|(i, keep)| (indices[i].0, keep))
                            .collect();
                        let width = left + width;
                        used += width;
                        keeps.push((index, AtomKeep::BoxPrefix { keeps: child_keeps }));
                    }
                }
                _ => {} // Atomic inline content is kept whole or hidden.
            }
            break;
        }
        (keeps, used)
    }
}
