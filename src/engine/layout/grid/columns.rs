//! Resolve intrinsic track contributions before assigning leftover space to fr tracks.
//! https://www.w3.org/TR/css-grid-1/#algo-content
use super::*;
#[cfg(test)]
mod tests;

struct TrackSize {
    base: f32,
    limit: f32,
    flex: f32,
    stretch: bool,
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn intrinsic_grid_columns(
        &mut self,
        tracks: &[GridTrack],
        items: &[GridItemPlacement],
        width: f32,
        gap: f32,
        font: f32,
    ) -> Vec<f32> {
        let available = (width - gap * tracks.len().saturating_sub(1) as f32).max(0.0);
        let mut sizes = tracks
            .iter()
            .map(|track| initial(track, available, font))
            .collect::<Vec<_>>();
        let mut items = items.iter().collect::<Vec<_>>();
        items.sort_by_key(|item| item.column_end - item.column);
        for item in items {
            let range = item.column..item.column_end;
            // Flexible spanning items are handled during flex expansion, not by
            // inflating a neighboring intrinsic sidebar with the whole heading.
            if range.len() > 1 && sizes[range.clone()].iter().any(|s| s.flex > 0.0) {
                continue;
            }
            if !tracks[range.clone()]
                .iter()
                .any(|t| intrinsic_min(t).is_some() || intrinsic_max(t).is_some())
            {
                continue;
            }
            let (lo, hi) = self.float_intrinsic_widths(&item.node, available);
            let spacing = gap * range.len().saturating_sub(1) as f32;
            for maximum in [false, true] {
                let contributors = range
                    .clone()
                    .filter_map(|i| {
                        let max_content = if maximum {
                            intrinsic_max(&tracks[i])
                        } else {
                            intrinsic_min(&tracks[i])
                        }?;
                        Some((i, if max_content { hi } else { lo }))
                    })
                    .collect::<Vec<_>>();
                if contributors.is_empty() {
                    continue;
                }
                let requested = contributors
                    .iter()
                    .map(|(_, value)| *value)
                    .fold(0.0, f32::max);
                let current: f32 = sizes[range.clone()]
                    .iter()
                    .map(|s| if maximum { s.limit.max(s.base) } else { s.base })
                    .sum();
                let extra = (requested - spacing - current).max(0.0) / contributors.len() as f32;
                for (index, _) in contributors {
                    if maximum {
                        sizes[index].limit = sizes[index].limit.max(sizes[index].base) + extra;
                    } else {
                        sizes[index].base += extra;
                        sizes[index].limit = sizes[index].limit.max(sizes[index].base);
                    }
                }
            }
        }
        // Maximize finite tracks up to their growth limits before expanding flex.
        let mut remaining = (available - sizes.iter().map(|s| s.base).sum::<f32>()).max(0.0);
        loop {
            let growable = sizes
                .iter()
                .filter(|s| s.flex == 0.0 && s.limit - s.base > 0.001)
                .count();
            if growable == 0 || remaining <= 0.001 {
                break;
            }
            let share = remaining / growable as f32;
            let mut used = 0.0;
            for size in &mut sizes {
                if size.flex == 0.0 {
                    let growth = share.min((size.limit - size.base).max(0.0));
                    size.base += growth;
                    used += growth;
                }
            }
            remaining = (remaining - used).max(0.0);
        }
        // Freeze tracks whose intrinsic minimum is larger than their fr share,
        // then recompute the fraction for the remaining tracks.
        let mut active = sizes.iter().map(|s| s.flex > 0.0).collect::<Vec<_>>();
        loop {
            let factors: f32 = sizes
                .iter()
                .zip(&active)
                .filter(|(_, a)| **a)
                .map(|(s, _)| s.flex)
                .sum();
            if factors == 0.0 {
                break;
            }
            let fixed: f32 = sizes
                .iter()
                .zip(&active)
                .filter(|(_, a)| !**a)
                .map(|(s, _)| s.base)
                .sum();
            let fraction = (available - fixed).max(0.0) / factors.max(1.0);
            let mut froze = false;
            for (size, active) in sizes.iter().zip(&mut active) {
                if *active && size.base > size.flex * fraction {
                    *active = false;
                    froze = true;
                }
            }
            if !froze {
                for (size, active) in sizes.iter_mut().zip(&active) {
                    if *active {
                        size.base = size.base.max(size.flex * fraction);
                    }
                }
                break;
            }
        }
        remaining = (available - sizes.iter().map(|s| s.base).sum::<f32>()).max(0.0);
        let auto_count = sizes.iter().filter(|s| s.stretch).count();
        for size in &mut sizes {
            if size.stretch && auto_count > 0 {
                size.base += remaining / auto_count as f32;
            }
        }
        sizes.into_iter().map(|s| s.base).collect()
    }
}

fn intrinsic_min(track: &GridTrack) -> Option<bool> {
    match track {
        GridTrack::Auto | GridTrack::MinContent | GridTrack::Fraction(_) => Some(false),
        GridTrack::MaxContent => Some(true),
        GridTrack::MinMax(min, _) => intrinsic_min(min),
        GridTrack::Fixed(_) => None,
    }
}

fn intrinsic_max(track: &GridTrack) -> Option<bool> {
    match track {
        GridTrack::Auto | GridTrack::MaxContent => Some(true),
        GridTrack::MinContent => Some(false),
        GridTrack::MinMax(_, max) => intrinsic_max(max),
        _ => None,
    }
}

fn initial(track: &GridTrack, basis: f32, font: f32) -> TrackSize {
    match track {
        GridTrack::Fixed(length) => {
            let value = length.resolve(basis, font).unwrap_or(0.0).max(0.0);
            TrackSize {
                base: value,
                limit: value,
                flex: 0.0,
                stretch: false,
            }
        }
        GridTrack::Fraction(flex) => TrackSize {
            base: 0.0,
            limit: 0.0,
            flex: *flex,
            stretch: false,
        },
        GridTrack::MinMax(min, max) => {
            let lower = initial(min, basis, font);
            let mut upper = initial(max, basis, font);
            upper.base = lower.base;
            upper.limit = upper.limit.max(lower.base);
            upper
        }
        _ => TrackSize {
            base: 0.0,
            limit: 0.0,
            flex: 0.0,
            stretch: matches!(track, GridTrack::Auto),
        },
    }
}
