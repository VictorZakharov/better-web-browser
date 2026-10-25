//! Baseline-relative placement for line boxes holding `vertical-align` length shifts.
//!
//! Lines without a non-zero shift keep the engine's centred placement. A line with one
//! follows CSS 2.1 §10.8: baseline-relative atoms are positioned against the shared
//! baseline, shifted ones raised by their resolved offset, and the line box spans from the
//! highest atom top (or the strut's) to the lowest atom bottom (or the strut's). `top` and
//! `bottom` atoms are then aligned to the line box, which grows to fit them.

use super::*;

/// The per-formatting-context values a line needs beyond its atoms.
#[derive(Debug, Clone, Copy)]
pub(in crate::engine::layout) struct LineFormat {
    pub(in crate::engine::layout) align: TextAlign,
    pub(in crate::engine::layout) strut: Strut,
    /// The line ends at a forced break (`<br>`), so it is never treated as empty.
    pub(in crate::engine::layout) broken: bool,
}

/// The block container's zero-width strut: its first font with the container's line
/// height, split around the baseline with half-leading on each side (CSS 2.1 §10.8.1).
#[derive(Debug, Clone, Copy, Default)]
pub(in crate::engine::layout) struct Strut {
    pub(in crate::engine::layout) ascent: f32,
    pub(in crate::engine::layout) descent: f32,
    /// Approximate x-height of the container's font, for `middle`.
    pub(in crate::engine::layout) x_height: f32,
}

/// The measurer exposes no ascent metric, so the content area is taken as the em box with
/// a typical 0.8/0.2 split. Only lines that contain shifted atoms use this split.
const ASCENT_FRACTION: f32 = 0.8;

fn split_around_baseline(font_size: f32, line_height: f32) -> (f32, f32) {
    let ascent = ASCENT_FRACTION * font_size + (line_height - font_size) / 2.0;
    (ascent, line_height - ascent)
}

impl Strut {
    pub(in crate::engine::layout) fn new(font_size: f32, line_height: f32) -> Self {
        let (ascent, descent) = split_around_baseline(font_size, line_height);
        Self {
            ascent,
            descent,
            x_height: font_size / 2.0,
        }
    }
}

/// The line box of a baseline-placed line: the distance from its top to the baseline, and
/// its height.
#[derive(Debug, Clone, Copy)]
pub(in crate::engine::layout) struct BaselineLine {
    above: f32,
    pub(in crate::engine::layout) height: f32,
}

/// Where an atom sits vertically on a baseline-placed line.
enum Placement {
    /// Its ascent above the baseline, after any shift.
    Baseline(f32),
    Top,
    Bottom,
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(in crate::engine::layout) fn line_format(&mut self, style: &ComputedStyle) -> LineFormat {
        let line_height = self.used_line_height(style);
        LineFormat {
            align: style.text_align.physical(style.direction),
            strut: Strut::new(style.font_size, line_height),
            broken: false,
        }
    }

    /// Returns `None` when no atom on the line carries a non-zero length shift.
    pub(in crate::engine::layout) fn baseline_line(
        &self,
        line: &[MeasuredAtom<'_>],
        strut: Strut,
    ) -> Option<BaselineLine> {
        let shifted = line.iter().any(|measured| {
            let (align, font_size, line_height) = self.atom_alignment(measured.atom);
            matches!(align, VerticalAlign::Shift(shift)
                if shift.resolve(font_size, line_height) != 0.0)
        });
        if !shifted {
            return None;
        }
        let (mut above, mut below) = (strut.ascent, strut.descent);
        let (mut top_height, mut bottom_height) = (0.0_f32, 0.0_f32);
        for measured in line {
            match self.placement(measured, strut) {
                Placement::Baseline(ascent) => {
                    above = above.max(ascent);
                    below = below.max(measured.height - ascent);
                }
                Placement::Top => top_height = top_height.max(measured.height),
                Placement::Bottom => bottom_height = bottom_height.max(measured.height),
            }
        }
        // A taller top-aligned box extends the line downward, a bottom-aligned one upward.
        below += (top_height - (above + below)).max(0.0);
        above += (bottom_height - (above + below)).max(0.0);
        Some(BaselineLine {
            above,
            height: (above + below).max(0.0),
        })
    }

    /// The atom's top relative to the top of `line`.
    pub(in crate::engine::layout) fn baseline_top(
        &self,
        measured: &MeasuredAtom<'_>,
        strut: Strut,
        line: BaselineLine,
    ) -> f32 {
        match self.placement(measured, strut) {
            Placement::Baseline(ascent) => line.above - ascent,
            Placement::Top => 0.0,
            Placement::Bottom => line.height - measured.height,
        }
    }

    fn placement(&self, measured: &MeasuredAtom<'_>, strut: Strut) -> Placement {
        let (align, font_size, line_height) = self.atom_alignment(measured.atom);
        let height = measured.height;
        let ascent = match align {
            VerticalAlign::Top => return Placement::Top,
            VerticalAlign::Bottom => return Placement::Bottom,
            // The box's midpoint sits half the parent's x-height above the baseline.
            VerticalAlign::Middle => height / 2.0 + strut.x_height / 2.0,
            VerticalAlign::Baseline | VerticalAlign::Shift(_) => self.atom_ascent(measured),
        };
        let raise = match align {
            VerticalAlign::Shift(shift) => shift.resolve(font_size, line_height),
            _ => 0.0,
        };
        Placement::Baseline(ascent + raise)
    }

    /// The distance from the atom's top to its own baseline (CSS 2.1 §10.8.1).
    fn atom_ascent(&self, measured: &MeasuredAtom<'_>) -> f32 {
        let node = match measured.atom {
            // Text and inline boxes centre their content area in their own line height.
            InlineAtom::Text { font, .. } => {
                return split_around_baseline(font.size, measured.height).0;
            }
            InlineAtom::InlineBox { style, .. } => {
                return split_around_baseline(style.font_size, measured.height).0;
            }
            InlineAtom::BlockBox { node, .. } => Some(node.clone()),
            InlineAtom::Control { spec, .. } => self.styles.node(spec.node_id),
            // Replaced elements sit on their bottom margin edge.
            _ => None,
        };
        let Some(node) = node else {
            return measured.height;
        };
        let style = self.styles.get(&node);
        // An inline-block's baseline is that of its last line box, or its bottom margin
        // edge when it has none or does not show overflow. The measurer has no metrics for
        // the last line, so it is taken to use the box's own font and line height.
        if style.overflow_establishes_formatting_context() || !self.has_line_boxes(&node, 0) {
            return measured.height;
        }
        let edge = |length: &Length| length.resolve(0.0, style.font_size).unwrap_or(0.0);
        let bottom = edge(&style.margin.bottom)
            + edge(&style.border_width.bottom)
            + edge(&style.padding.bottom);
        let descent = split_around_baseline(style.font_size, own_line_height(style)).1;
        (measured.height - bottom - descent).max(0.0)
    }

    /// Whether in-flow content of `node` creates a non-empty line box.
    fn has_line_boxes(&self, node: &NodeRef, depth: usize) -> bool {
        depth < 32
            && self
                .box_children(node)
                .iter()
                .any(|child| match &child.data {
                    // Only collapsible white space is empty; `&nbsp;` still makes a line box.
                    NodeData::Text(text) => text
                        .borrow()
                        .chars()
                        .any(|c| !matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}')),
                    _ => {
                        let style = self.styles.get(child);
                        if style.display == Display::None
                            || style.float != Float::None
                            || matches!(style.position, Position::Absolute | Position::Fixed)
                        {
                            false
                        } else if style.display == Display::Inline || is_block_level(style.display)
                        {
                            self.has_line_boxes(child, depth + 1)
                        } else {
                            // Atomic inline-level content always occupies a line box.
                            true
                        }
                    }
                })
    }

    /// The atom's own `vertical-align` with the font size and line height its
    /// `em` and percentage shifts resolve against. Text inherits its parent's baseline.
    fn atom_alignment(&self, atom: &InlineAtom) -> (VerticalAlign, f32, f32) {
        let node = match atom {
            InlineAtom::BlockBox { node, .. } => Some(node.clone()),
            InlineAtom::Image { node_id, .. } => self.styles.node(*node_id),
            InlineAtom::Control { spec, .. } => self.styles.node(spec.node_id),
            InlineAtom::InlineBox { style, .. } => {
                return (
                    style.vertical_align,
                    style.font_size,
                    own_line_height(style),
                );
            }
            _ => None,
        };
        node.map_or((VerticalAlign::Baseline, 0.0, 0.0), |node| {
            let style = self.styles.get(&node);
            (
                style.vertical_align,
                style.font_size,
                own_line_height(style),
            )
        })
    }
}

fn own_line_height(style: &ComputedStyle) -> f32 {
    if style.normal_line_height() {
        style.font_size * 1.2
    } else {
        style.line_height
    }
}
