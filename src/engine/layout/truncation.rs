//! Shared text-truncation policy and grapheme-safe ellipsis fitting.
//!
//! Both truncation mechanisms reuse these primitives but trigger differently:
//! single-line `text-overflow: ellipsis` reacts to a line overflowing its
//! available width, while legacy `-webkit-line-clamp` limits the number of
//! painted line boxes and forces a marker on the final visible line when more
//! content follows. See CSS Overflow 3
//! (https://drafts.csswg.org/css-overflow-3/#text-overflow) and CSS Overflow 4
//! legacy compatibility (https://drafts.csswg.org/css-overflow-4/#legacy-compatibility).
//!
//! Geometry contract: truncation shortens only paint. Fragment and element
//! geometry keep the authored prefix so DOM offsets, Range mapping and link
//! ownership stay intact; the synthetic marker (`…`, U+2026) never gains source
//! clusters. Fitting probes use `measure()` alone, so geometry-only and
//! painted layout agree without depending on `shape()` payloads.

use super::*;
use unicode_segmentation::UnicodeSegmentation;

/// The synthetic overflow marker. Shaped text, not three dots.
pub(super) const ELLIPSIS: &str = "…";

/// When truncation applies to one block formatting context.
#[derive(Debug, Clone)]
pub(super) struct TruncationPolicy {
    /// `text-overflow: ellipsis` with non-wrapping, non-visible overflow.
    pub single_line_ellipsis: bool,
    /// Legacy clamp line budget. `None` means clamping is inactive.
    pub max_lines: Option<u32>,
    pub container_font: FontSpec,
    pub container_color: Color,
    pub container_visible: bool,
}
impl TruncationPolicy {
    pub(super) fn for_style(style: &ComputedStyle) -> Self {
        Self {
            single_line_ellipsis: style.ellipsis_on_overflow(),
            max_lines: style.clamp_max_lines(),
            container_font: FontSpec::from_style(style),
            container_color: style.color,
            container_visible: style.visibility,
        }
    }
}

/// Mutable line budget shared by every inline flush of one block container.
/// Fresh per formatting context, so a clamp can never leak into a sibling or a
/// nested independent box.
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

/// How one line atom survives truncation. Indices after the last kept atom are
/// dropped along with every later line when clamping.
#[derive(Debug, Clone)]
pub(super) enum AtomKeep {
    Whole,
    TextPrefix {
        len: usize,
        width: f32,
    },
    BoxPrefix {
        keeps: Vec<(usize, AtomKeep)>,
        width: f32,
        children_width: f32,
    },
}

/// Planned truncation inside one inline box.
#[derive(Debug, Clone, Default)]
struct BoxPlan {
    keeps: Vec<(usize, AtomKeep)>,
    width: f32,
    children_width: f32,
}

/// Ownership styling for the marker, taken from the last kept text.
#[derive(Debug, Clone)]
struct MarkerSource {
    font: FontSpec,
    color: Color,
    link: Option<String>,
    node_id: Option<NodeId>,
    visible: bool,
    height: f32,
    content_height: f32,
}

/// The painted marker trailing a truncated line.
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

/// A complete truncation decision for one line box.
#[derive(Debug, Clone)]
pub(super) struct RowPlan {
    pub keeps: Vec<(usize, AtomKeep)>,
    pub marker: TruncationMarker,
    /// Kept content plus marker: the aligned line width.
    pub width: f32,
}

/// Paint-time view of a truncated inline box: the kept child indices with the
/// shrunk border box and kept content width derived during planning.
#[derive(Debug, Clone, Copy)]
pub(super) struct BoxPaint<'a> {
    pub keeps: &'a [(usize, AtomKeep)],
    pub border_box_width: f32,
    pub children_width: f32,
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    /// Marker width in one font, measured rather than assumed.
    fn marker_width(&mut self, font: &FontSpec) -> f32 {
        self.measurer.measure(ELLIPSIS, font).0
    }

    /// Largest marker width over every text font on the line (including nested
    /// inline boxes) and the container font. Reserving the maximum keeps the
    /// final marker inside the available width whatever the boundary font is.
    fn marker_reserve(&mut self, line: &[MeasuredAtom<'_>], policy: &TruncationPolicy) -> f32 {
        fn collect(atom: &InlineAtom, fonts: &mut Vec<FontSpec>) {
            match atom {
                InlineAtom::Text { font, .. } if !fonts.contains(font) => {
                    fonts.push(font.clone());
                }
                InlineAtom::InlineBox { children, .. } => {
                    for child in children {
                        collect(child, fonts);
                    }
                }
                _ => {}
            }
        }
        let mut fonts = vec![policy.container_font.clone()];
        for measured in line {
            collect(measured.atom, &mut fonts);
        }
        fonts
            .iter()
            .map(|font| self.marker_width(font))
            .fold(0.0_f32, f32::max)
    }

    /// Longest grapheme-boundary prefix of `text` fitting `budget` on its own.
    /// Binary search keeps probing logarithmic; trailing spaces are trimmed so
    /// the marker never floats after a gap.
    fn fit_text_prefix(&mut self, text: &str, font: &FontSpec, budget: f32) -> (usize, f32) {
        if text.is_empty() {
            return (0, 0.0);
        }
        let (full_width, _) = self.measurer.measure(text, font);
        if full_width <= budget {
            return (text.len(), full_width);
        }
        let boundaries: Vec<usize> = text
            .grapheme_indices(true)
            .map(|(index, _)| index)
            .chain(std::iter::once(text.len()))
            .collect();
        let mut low = 0_usize;
        let mut high = boundaries.len() - 1;
        while low + 1 < high {
            let mid = (low + high) / 2;
            let width = self.measurer.measure(&text[..boundaries[mid]], font).0;
            if width <= budget {
                low = mid;
            } else {
                high = mid;
            }
        }
        let len = text[..boundaries[low]].trim_end_matches([' ', '\t']).len();
        let width = self.measurer.measure(&text[..len], font).0;
        (len, width)
    }

    /// Greedy single-row truncation plan for one line box. Returns `None` when
    /// even the marker cannot fit, in which case the caller leaves the line
    /// untruncated (matching Chrome, which then just clips the content).
    pub(super) fn plan_truncated_row(
        &mut self,
        line: &[MeasuredAtom<'_>],
        available: f32,
        containing: f32,
        policy: &TruncationPolicy,
    ) -> Option<RowPlan> {
        let available = available.max(0.0);
        let reserve = self.marker_reserve(line, policy);
        if reserve > available || reserve <= 0.0 {
            return None;
        }
        let mut keeps = Vec::new();
        let mut kept_width = 0.0_f32;
        let mut remaining = available - reserve;
        for (index, measured) in line.iter().enumerate() {
            match measured.atom {
                InlineAtom::Text { font, .. } => {
                    let text = measured.text.unwrap_or_default();
                    let (width, _) = self.measurer.measure(text, font);
                    if width <= remaining {
                        keeps.push((index, AtomKeep::Whole));
                        kept_width += width;
                        remaining -= width;
                    } else {
                        let (len, prefix_width) = self.fit_text_prefix(text, font, remaining);
                        kept_width += prefix_width;
                        keeps.push((
                            index,
                            AtomKeep::TextPrefix {
                                len,
                                width: prefix_width,
                            },
                        ));
                        break;
                    }
                }
                InlineAtom::InlineBox {
                    children, style, ..
                } => {
                    if measured.width <= remaining {
                        keeps.push((index, AtomKeep::Whole));
                        kept_width += measured.width;
                        remaining -= measured.width;
                    } else if let Some(box_plan) = self.plan_inline_box_row(
                        children, style, measured, remaining, containing, policy,
                    ) {
                        kept_width += box_plan.width;
                        keeps.push((
                            index,
                            AtomKeep::BoxPrefix {
                                keeps: box_plan.keeps,
                                width: box_plan.width,
                                children_width: box_plan.children_width,
                            },
                        ));
                        break;
                    } else {
                        break;
                    }
                }
                // Atomic content is indivisible and belongs to an independent
                // context (BlockBox) or has no text to shape: keep it only
                // when it still fits, otherwise drop it and everything after.
                _ => {
                    if measured.width <= remaining {
                        keeps.push((index, AtomKeep::Whole));
                        kept_width += measured.width;
                        remaining -= measured.width;
                    } else {
                        break;
                    }
                }
            }
        }
        let source = self.source_for_keeps(line, &keeps, containing);
        // A lone marker with no kept content carries no information; leave the
        // line untruncated so overflow clipping applies (Chrome narrow-box behavior).
        if kept_width <= 0.0 {
            return None;
        }
        let marker = self.marker_for_source(source, policy);
        Some(RowPlan {
            keeps,
            width: kept_width + marker.width,
            marker,
        })
    }

    /// Single-row truncation inside one inline box. `None` means no child fits
    /// and the whole box must be dropped by the caller.
    fn plan_inline_box_row(
        &mut self,
        children: &[InlineAtom],
        style: &ComputedStyle,
        measured: &MeasuredAtom<'_>,
        budget: f32,
        containing: f32,
        policy: &TruncationPolicy,
    ) -> Option<BoxPlan> {
        let metrics = self.measure_inline_box(measured.atom, children, style, containing);
        let content_width =
            (metrics.border_box_width - metrics.border.horizontal() - metrics.padding.horizontal())
                .max(0.0);
        let mut row = Vec::new();
        let mut child_index = Vec::new();
        for (index, child) in children.iter().enumerate() {
            if matches!(child, InlineAtom::Break) {
                continue;
            }
            child_index.push(index);
            row.push(self.measure_atom(child, index == 0, content_width));
        }
        let plan = self.plan_truncated_row(&row, budget, content_width, policy)?;
        if plan.keeps.is_empty() {
            return None;
        }
        let kept_children_width = plan.width - plan.marker.width;
        let insets = metrics.border.horizontal() + metrics.padding.horizontal();
        // A specified width wins over content, exactly as in measurement; only
        // auto boxes shrink to their kept content, never beyond the full box.
        let shrunk = resolve_outer_size(
            style.width,
            containing,
            style.font_size,
            insets,
            style.box_sizing,
        )
        .unwrap_or(kept_children_width + insets)
        .min(metrics.border_box_width)
        .max(0.0);
        // Row positions skip `Break` children; translate back to child indices
        // for the paint walk.
        let keeps = plan
            .keeps
            .into_iter()
            .map(|(position, keep)| (child_index[position], keep))
            .collect();
        Some(BoxPlan {
            keeps,
            width: shrunk,
            children_width: kept_children_width,
        })
    }

    /// Last kept text in a keep list, recursing into truncated inline boxes.
    /// Child measurements hit the same cache entries the planner populated.
    fn source_for_keeps(
        &mut self,
        line: &[MeasuredAtom<'_>],
        keeps: &[(usize, AtomKeep)],
        containing: f32,
    ) -> Option<MarkerSource> {
        for (index, keep) in keeps.iter().rev() {
            let measured = &line[*index];
            match (measured.atom, keep) {
                (
                    InlineAtom::Text {
                        font,
                        color,
                        link,
                        node_id,
                        visible,
                        ..
                    },
                    AtomKeep::Whole | AtomKeep::TextPrefix { .. },
                ) => {
                    return Some(MarkerSource {
                        font: font.clone(),
                        color: *color,
                        link: link.clone(),
                        node_id: *node_id,
                        visible: *visible,
                        height: measured.height,
                        content_height: measured.content_height,
                    });
                }
                (
                    InlineAtom::InlineBox {
                        children, style, ..
                    },
                    AtomKeep::BoxPrefix { keeps, .. },
                ) => {
                    let metrics =
                        self.measure_inline_box(measured.atom, children, style, containing);
                    let content_width = (metrics.border_box_width
                        - metrics.border.horizontal()
                        - metrics.padding.horizontal())
                    .max(0.0);
                    let mut row = Vec::new();
                    let mut child_index = Vec::new();
                    for (child_position, child) in children.iter().enumerate() {
                        if matches!(child, InlineAtom::Break) {
                            continue;
                        }
                        child_index.push(child_position);
                        row.push(self.measure_atom(child, child_position == 0, content_width));
                    }
                    // Stored keeps address child indices; translate to row positions.
                    let translated: Vec<(usize, AtomKeep)> = keeps
                        .iter()
                        .filter_map(|(child_position, keep)| {
                            child_index
                                .iter()
                                .position(|index| index == child_position)
                                .map(|position| (position, keep.clone()))
                        })
                        .collect();
                    if let Some(source) = self.source_for_keeps(&row, &translated, content_width) {
                        return Some(source);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Marker ownership follows the last kept text: its font, color, link and
    /// node keep the marker hit-testable exactly like the content it replaces.
    /// With no kept text, the container style supplies the marker.
    fn marker_for_source(
        &mut self,
        source: Option<MarkerSource>,
        policy: &TruncationPolicy,
    ) -> TruncationMarker {
        if let Some(source) = source {
            let width = self.marker_width(&source.font);
            TruncationMarker {
                width,
                font: source.font,
                color: source.color,
                link: source.link,
                node_id: source.node_id,
                visible: source.visible,
                ref_height: source.height,
                ref_content_height: source.content_height,
            }
        } else {
            let width = self.marker_width(&policy.container_font);
            TruncationMarker {
                width,
                font: policy.container_font.clone(),
                color: policy.container_color,
                link: None,
                node_id: None,
                visible: policy.container_visible,
                ref_height: policy.container_font.size,
                ref_content_height: policy.container_font.size,
            }
        }
    }
}
