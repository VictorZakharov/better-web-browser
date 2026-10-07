// Copyright 2026 Breeze contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Element-owned textLength adjustment, not independent paint-span adjustment.
//! SVG 2 §11.5: resolve descendants before parents; a resolved descendant is
//! one spacing unit for its ancestor. Shaping, bidi and outlines stay upstream.

use super::{GlyphCluster, LengthAdjust, Text};

struct Address {
    chunk: usize,
    cluster: usize,
    character: usize,
}

pub(super) fn adjust(text: &Text, shaped: &mut [Vec<GlyphCluster>]) {
    if text.length_adjustments.is_empty() {
        return;
    }
    let mut addresses = Vec::new();
    let mut offset = 0;
    for (chunk_index, (chunk, clusters)) in text.chunks.iter().zip(shaped.iter()).enumerate() {
        let byte_indices: Vec<_> = chunk.text.char_indices().map(|(byte, _)| byte).collect();
        for (index, cluster) in clusters.iter().enumerate() {
            if let Ok(character) = byte_indices.binary_search(&cluster.byte_idx.value()) {
                addresses.push(Address {
                    chunk: chunk_index,
                    cluster: index,
                    character: offset + character,
                });
            }
        }
        offset += byte_indices.len();
    }
    let parents: Vec<_> = text
        .length_adjustments
        .iter()
        .enumerate()
        .map(|(index, child)| {
            text.length_adjustments
                .iter()
                .enumerate()
                .skip(index + 1)
                .find(|(_, parent)| parent.start <= child.start && parent.end >= child.end)
                .map(|(index, _)| index)
        })
        .collect();
    let mut children = vec![Vec::new(); parents.len()];
    for (index, parent) in parents.iter().enumerate() {
        if let Some(parent) = parent {
            let range = &text.length_adjustments[index];
            children[*parent].push((index, range.start..range.end));
        }
    }
    for ranges in &mut children {
        ranges.sort_by_key(|(_, range)| range.start);
    }
    for (range_index, range) in text.length_adjustments.iter().enumerate() {
        let selected: Vec<_> = addresses
            .iter()
            .filter(|address| (range.start..range.end).contains(&address.character))
            .collect();
        if selected.is_empty() {
            continue;
        }
        let owners: Vec<_> = selected
            .iter()
            .map(|address| owner_at(&children[range_index], address.character))
            .collect();
        let mut width = 0.0;
        for address in &selected {
            width += shaped[address.chunk][address.cluster].advance;
        }
        let last = selected.last().unwrap();
        let tail = &shaped[last.chunk][last.cluster];
        // Trailing letter/word spacing is not part of the last glyph's extent.
        width += tail.width - tail.advance;
        if !width.is_finite() || width <= 0.0 {
            continue;
        }
        if range.adjust == LengthAdjust::SpacingAndGlyphs {
            // A child's explicit length owns its internal adjustment, including
            // when the parent scales glyphs rather than adding spacing (SVG 2
            // textLength attribute definition). Only the remaining advances
            // absorb the parent's requested length.
            let fixed_width: f32 = selected
                .iter()
                .enumerate()
                .filter(|(index, _)| owners[*index].is_some())
                .map(|(index, address)| {
                    let cluster = &shaped[address.chunk][address.cluster];
                    if index + 1 == selected.len() {
                        cluster.width
                    } else {
                        cluster.advance
                    }
                })
                .sum();
            let adjustable_width = width - fixed_width;
            let factor = (range.length - fixed_width) / adjustable_width;
            // An ancestor cannot override a wholly resolved child or shrink
            // below its fixed descendants. Preserve child ownership rather
            // than reflecting glyphs to satisfy an overspecified parent.
            if adjustable_width <= 0.0 || !factor.is_finite() || factor < 0.0 {
                continue;
            }
            for (address, owner) in selected.into_iter().zip(owners) {
                if owner.is_some() {
                    continue;
                }
                let cluster = &mut shaped[address.chunk][address.cluster];
                cluster.transform = cluster.transform.pre_scale(factor, 1.0);
                cluster.width *= factor;
                cluster.advance *= factor;
            }
        } else {
            let boundaries = spacing_boundaries(&owners);
            if boundaries.is_empty() {
                continue;
            }
            let extra = (range.length - width) / boundaries.len() as f32;
            for index in boundaries {
                let address = selected[index];
                shaped[address.chunk][address.cluster].advance += extra;
            }
        }
    }
}

/// Direct descendant ranges are disjoint in rendered-character order. Index
/// them once, rather than scanning every adjustment for every shaped glyph.
fn owner_at(ranges: &[(usize, std::ops::Range<usize>)], character: usize) -> Option<usize> {
    let position = ranges.partition_point(|(_, range)| range.start <= character);
    let (owner, range) = ranges.get(position.checked_sub(1)?)?;
    range.contains(&character).then_some(*owner)
}

/// Distinct glyph clusters are units unless a resolved child owns both.
/// No trailing gap: the final typographic character is shifted by the sum.
fn spacing_boundaries(owners: &[Option<usize>]) -> Vec<usize> {
    owners
        .windows(2)
        .enumerate()
        .filter_map(|(index, pair)| (pair[0].is_none() || pair[0] != pair[1]).then_some(index))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::spacing_boundaries;

    #[test]
    fn painted_spans_are_not_atomic_spacing_units() {
        assert_eq!(spacing_boundaries(&[None, None, None]), [0, 1]);
    }

    #[test]
    fn resolved_children_keep_their_internal_spacing() {
        assert_eq!(spacing_boundaries(&[None, Some(2), Some(2), None]), [0, 2]);
        assert_eq!(
            spacing_boundaries(&[Some(1), Some(1), Some(2), Some(2)]),
            [1]
        );
        assert!(spacing_boundaries(&[Some(1), Some(1)]).is_empty());
    }

    #[test]
    fn empty_and_single_clusters_have_no_adjustable_gap() {
        assert!(spacing_boundaries(&[]).is_empty());
        assert!(spacing_boundaries(&[None]).is_empty());
    }

    #[test]
    fn indexed_children_keep_boundaries_holes_and_logical_order() {
        let ranges = [(4, 2..5), (1, 8..10), (9, 12..14)];
        let expected = [
            None,
            None,
            Some(4),
            Some(4),
            Some(4),
            None,
            None,
            None,
            Some(1),
            Some(1),
            None,
            None,
            Some(9),
            Some(9),
            None,
        ];
        for (character, expected) in expected.into_iter().enumerate() {
            assert_eq!(super::owner_at(&ranges, character), expected);
        }
        assert_eq!(super::owner_at(&ranges, usize::MAX), None);
        assert_eq!(super::owner_at(&[], 0), None);
    }
}
