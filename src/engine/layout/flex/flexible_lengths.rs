//! CSS Flexbox 9.7: distribute free space, freeze violations, then redistribute.

#[derive(Clone, Copy)]
pub(super) struct FlexibleSize {
    pub base: f32,
    pub minimum: f32,
    pub maximum: f32,
    pub grow: f32,
    pub shrink: f32,
}

impl FlexibleSize {
    fn clamp(self, value: f32) -> f32 {
        value.min(self.maximum).max(self.minimum).max(0.0)
    }
}

pub(super) fn resolve(items: &[FlexibleSize], available: Option<f32>) -> Vec<f32> {
    let hypothetical: Vec<_> = items.iter().map(|item| item.clamp(item.base)).collect();
    let Some(available) = available else {
        return hypothetical;
    };
    let growing = hypothetical.iter().sum::<f32>() < available;
    let factor = |item: &FlexibleSize| if growing { item.grow } else { item.shrink };
    let mut targets = items.iter().map(|item| item.base).collect::<Vec<_>>();
    let mut frozen: Vec<_> = items
        .iter()
        .zip(&hypothetical)
        .map(|(item, size)| {
            factor(item) == 0.0
                || if growing {
                    item.base > *size
                } else {
                    item.base < *size
                }
        })
        .collect();
    for (index, fixed) in frozen.iter().enumerate() {
        if *fixed {
            targets[index] = hypothetical[index];
        }
    }
    let remaining = |targets: &[f32], frozen: &[bool]| {
        available
            - items
                .iter()
                .enumerate()
                .map(|(i, item)| if frozen[i] { targets[i] } else { item.base })
                .sum::<f32>()
    };
    let initial = remaining(&targets, &frozen);
    // Each pass freezes at least one item, even when roundoff makes violations cancel.
    for _ in 0..items.len() {
        if frozen.iter().all(|fixed| *fixed) {
            break;
        }
        let mut free = remaining(&targets, &frozen);
        let sum = items
            .iter()
            .zip(&frozen)
            .filter(|(_, fixed)| !**fixed)
            .map(|(item, _)| factor(item))
            .sum::<f32>();
        if sum < 1.0 && (initial * sum).abs() < free.abs() {
            free = initial * sum;
        }
        let weight = items
            .iter()
            .zip(&frozen)
            .filter(|(_, fixed)| !**fixed)
            .map(|(item, _)| {
                if growing {
                    item.grow
                } else {
                    item.shrink * item.base
                }
            })
            .sum::<f32>();
        let mut violations = vec![0.0; items.len()];
        for (index, item) in items.iter().enumerate().filter(|(i, _)| !frozen[*i]) {
            let share = if weight > 0.0 {
                free * if growing {
                    item.grow
                } else {
                    item.shrink * item.base
                } / weight
            } else {
                0.0
            };
            let tentative = item.base + share;
            targets[index] = item.clamp(tentative);
            violations[index] = targets[index] - tentative;
        }
        let total: f32 = violations.iter().sum();
        for (index, violation) in violations.iter().enumerate() {
            if total.abs() < 0.001
                || (total > 0.0 && *violation > 0.0)
                || (total < 0.0 && *violation < 0.0)
            {
                frozen[index] = true;
            }
        }
    }
    targets
}

#[cfg(test)]
mod tests {
    use super::*;
    fn size(base: f32) -> FlexibleSize {
        FlexibleSize {
            base,
            minimum: 0.0,
            maximum: f32::INFINITY,
            grow: 1.0,
            shrink: 1.0,
        }
    }
    #[test]
    fn redistributes_after_maximum_or_minimum_violation() {
        let mut first = size(100.0);
        first.maximum = 120.0;
        assert_eq!(resolve(&[first, size(100.0)], Some(400.0)), [120.0, 280.0]);
        first.maximum = f32::INFINITY;
        first.minimum = 90.0;
        assert_eq!(resolve(&[first, size(100.0)], Some(120.0)), [90.0, 30.0]);
    }
    #[test]
    fn fractional_grow_can_leave_free_space_for_alignment() {
        let mut first = size(100.0);
        first.grow = 0.25;
        assert_eq!(resolve(&[first, first], Some(400.0)), [150.0, 150.0]);
    }
    #[test]
    fn indefinite_size_does_not_distribute_or_resolve_percentages() {
        assert_eq!(resolve(&[size(20.0), size(30.0)], None), [20.0, 30.0]);
    }
}
