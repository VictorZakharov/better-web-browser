use super::*;
use resvg::tiny_skia::{PathBuilder, Rect};

// Independently replay the previous full-ROI algorithm. Exact equality here
// protects samples/holes/clipped winding, not merely a few interior pixels.
fn full(region: &Region, path: &Path, rule: FillRule) -> Vec<u8> {
    let pixels = region.pixels().unwrap();
    let mut scale = 1u32;
    if region.antialias {
        while scale < 4
            && pixels * (scale * 2).pow(2) as usize <= 8 * 1024 * 1024
            && region.width * scale * 2 < 8192
            && region.height * scale * 2 < 8192
        {
            scale *= 2;
        }
    }
    let mut mask = Mask::new(region.width * scale, region.height * scale).unwrap();
    mask.fill_path(
        path,
        rule,
        region.antialias,
        Transform::from_scale(scale as f32, scale as f32)
            .pre_translate(-(region.left as f32), -(region.top as f32)),
    );
    let samples = mask.take();
    (0..region.height)
        .flat_map(|y| {
            let samples = &samples;
            (0..region.width).map(move |x| {
                let mut sum = 0u32;
                for dy in 0..scale {
                    for dx in 0..scale {
                        sum += u32::from(
                            samples[((y * scale + dy) * region.width * scale + x * scale + dx)
                                as usize],
                        );
                    }
                }
                ((sum + scale * scale / 2) / (scale * scale)) as u8
            })
        })
        .collect()
}

fn curve(x: f32, y: f32) -> Path {
    let mut builder = PathBuilder::new();
    builder.move_to(x + 1.25, y + 3.75);
    builder.cubic_to(
        x + 18.125,
        y - 4.5,
        x + 29.25,
        y + 17.375,
        x + 6.5,
        y + 23.5,
    );
    builder.quad_to(x - 6.75, y + 11.5, x + 1.25, y + 3.75);
    builder.close();
    builder.push_circle(x + 9.25, y + 9.5, 2.125);
    builder.finish().unwrap()
}

#[test]
fn bounded_crop_retains_every_full_roi_sample_for_curves_holes_and_edges() {
    for antialias in [false, true] {
        for (left, top) in [(0, 0), (-17, -11), (301, 209)] {
            let region = Region {
                width: 97,
                height: 81,
                left,
                top,
                antialias,
            };
            for (x, y) in [(-25.5, -13.75), (0.0, 0.0), (19.25, 21.75), (87.5, 71.5)] {
                let path = curve(left as f32 + x, top as f32 + y);
                for rule in [FillRule::Winding, FillRule::EvenOdd] {
                    let actual = region.rasterize(Some(&path), rule).unwrap();
                    let expected = full(&region, &path, rule);
                    let mismatch = actual.iter().zip(&expected).position(|(a, b)| a != b);
                    assert!(
                        mismatch.is_none(),
                        "left={left} top={top} x={x} y={y} antialias={antialias} rule={rule:?} first mismatch={mismatch:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn crop_does_not_increase_sample_scale_when_original_roi_requires_native_aa() {
    // 8192 width forbids supersampling despite a very small actual path.
    let region = Region {
        width: 8192,
        height: 3,
        left: -3,
        top: 4,
        antialias: true,
    };
    let mut builder = PathBuilder::new();
    builder.push_rect(Rect::from_xywh(40.25, 4.125, 7.5, 1.375).unwrap());
    let path = builder.finish().unwrap();
    assert_eq!(
        region.rasterize(Some(&path), FillRule::Winding).unwrap(),
        full(&region, &path, FillRule::Winding)
    );
}

#[test]
fn empty_and_offscreen_geometry_return_owned_zero_masks_without_scan_allocation() {
    let region = Region {
        width: 512,
        height: 512,
        left: 0,
        top: 0,
        antialias: true,
    };
    let path = curve(800.0, 900.0);
    assert!(Crop::intersect(&region, &path).is_none());
    let mut first = region.rasterize(Some(&path), FillRule::Winding).unwrap();
    assert_eq!(first, vec![0; 512 * 512]);
    first[0] = 255;
    assert_eq!(
        region.rasterize(None, FillRule::Winding).unwrap(),
        vec![0; 512 * 512]
    );
}

#[test]
fn conservative_bounds_restrict_reduction_work_without_trimming_support() {
    let region = Region {
        width: 512,
        height: 512,
        left: 0,
        top: 0,
        antialias: true,
    };
    let mut builder = PathBuilder::new();
    builder.push_rect(Rect::from_xywh(20.5, 250.25, 460.0, 1.5).unwrap());
    let path = builder.finish().unwrap();
    let crop = Crop::intersect(&region, &path).unwrap();
    assert!(crop.width * crop.height < region.width * region.height / 50);
    assert_eq!(
        region.rasterize(Some(&path), FillRule::Winding).unwrap(),
        full(&region, &path, FillRule::Winding)
    );
}
