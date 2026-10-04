//! Bounded canvas requests may use smaller storage without changing content attributes.
//! WebGL §2, The Drawing Buffer: the actual extent is implementation dependent
//! when the requested extent cannot be satisfied. Texture storage stays strict.
use super::{Options, Surface};

pub(super) fn admitted(
    width: u32,
    height: u32,
    options: Options,
    budget: usize,
) -> Option<(u32, u32)> {
    let width = width.clamp(1, 4096);
    let height = height.clamp(1, 4096);
    let per_pixel = Surface::allocation_bytes(1, 1, options)?;
    let pixels = (budget / per_pixel).min(4 * 1024 * 1024);
    if pixels == 0 {
        return None;
    }
    if u64::from(width) * u64::from(height) <= pixels as u64 {
        return Some((width, height));
    }
    // Integer monotonic search avoids floating-point overflow/rounding for
    // UINT_MAX canvas attributes. Keep the bounded request's aspect ratio as
    // closely as this 1/4096 scale permits, with at least one pixel per axis.
    let extent = |scale: u32| {
        (
            (width * scale / 4096).max(1),
            (height * scale / 4096).max(1),
        )
    };
    let (mut lower, mut upper) = (0_u32, 4096_u32);
    while lower < upper {
        let middle = lower + (upper - lower).div_ceil(2);
        let (w, h) = extent(middle);
        if u64::from(w) * u64::from(h) <= pixels as u64 {
            lower = middle;
        } else {
            upper = middle - 1;
        }
    }
    Some(extent(lower))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::webgl::{ApiVersion, MAX_RESOURCE_BYTES};

    #[test]
    fn drawing_buffer_extent_preserves_exact_requests_including_zero_minimum() {
        for options in [
            Options::default(),
            Options {
                api: ApiVersion::Two,
                antialias: true,
                ..Options::default()
            },
        ] {
            assert_eq!(admitted(0, 0, options, MAX_RESOURCE_BYTES), Some((1, 1)));
            assert_eq!(
                admitted(192, 108, options, MAX_RESOURCE_BYTES),
                Some((192, 108))
            );
            assert_eq!(
                admitted(u32::MAX, 1, options, MAX_RESOURCE_BYTES),
                Some((4096, 1))
            );
        }
    }

    #[test]
    fn drawing_buffer_extent_accounts_for_multisample_and_all_attachment_storage() {
        for antialias in [false, true] {
            for depth in [false, true] {
                let options = Options {
                    api: ApiVersion::Two,
                    antialias,
                    depth,
                    ..Options::default()
                };
                for budget in [0, 1, 8, 20, 36, 100, 10_000, MAX_RESOURCE_BYTES] {
                    if let Some((width, height)) = admitted(u32::MAX, u32::MAX, options, budget) {
                        assert!((1..=4096).contains(&width));
                        assert!((1..=4096).contains(&height));
                        assert!(width as usize * height as usize <= 4 * 1024 * 1024);
                        assert!(
                            Surface::allocation_bytes(width, height, options).unwrap() <= budget
                        );
                    } else {
                        assert!(Surface::allocation_bytes(1, 1, options).unwrap() > budget);
                    }
                }
            }
        }
    }

    #[test]
    fn drawing_buffer_extent_uses_largest_admitted_quantized_scale() {
        let options = Options {
            api: ApiVersion::Two,
            antialias: true,
            ..Options::default()
        };
        let budget = 300 * 36;
        let (width, height) = admitted(40, 20, options, budget).unwrap();
        assert!(width >= 24 && height >= 12);
        assert!(width * height <= 300);
        assert!((width + 1) * (height + 1) > 300);
    }
}
