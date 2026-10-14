use super::*;
mod benchmark;

fn ring(widths: [f32; 4], radius: f32) -> Region {
    let rect = Rect {
        left: 0,
        top: 0,
        right: 64,
        bottom: 64,
    };
    let region = Region(unsafe {
        super::super::super::paint_primitives::border_ring(&rect, widths, radius)
    });
    assert!(!region.0.is_null());
    region
}

#[test]
fn region_scans_preserve_every_border_pixel_and_do_not_store_the_empty_box() {
    for radius in [0.0, 20.0] {
        for widths in [
            [8.0; 4],
            [3.0, 12.0, 5.0, 0.0],
            [2.0, 0.0, 0.0, 0.0],
            [0.0; 4],
        ] {
            let region = ring(widths, radius);
            let scans = rectangles(region.0).unwrap();
            let mut expected = vec![0; 65 * 65 * 4];
            super::super::shapes::fill_pixels(region.0, 65, 65, Color::WHITE, &mut expected)
                .unwrap();
            let mut actual = vec![0; 65 * 65 * 4];
            for rect in scans {
                for y in rect.top.max(0)..rect.bottom.min(65) {
                    for x in rect.left.max(0)..rect.right.min(65) {
                        let pixel = &mut actual[(y as usize * 65 + x as usize) * 4..][..4];
                        assert_eq!(pixel, &[0; 4], "coverage rectangles overlap");
                        pixel.fill(255);
                    }
                }
            }
            assert_eq!(actual, expected, "radius {radius}, widths {widths:?}");
        }
    }
    let rect = Rect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    };
    let region =
        Region(unsafe { super::super::super::paint_primitives::border_ring(&rect, [2.0; 4], 0.0) });
    assert_eq!(rectangles(region.0).unwrap().len(), 4);
}

fn render(device: &Device, target: &Target, paint: impl FnOnce()) -> Vec<u8> {
    unsafe {
        device.d2d.SetTarget(&target.bitmap);
        device.d2d.SetAntialiasMode(D2D1_ANTIALIAS_MODE_ALIASED);
        super::super::groups::identity(device);
        device.d2d.BeginDraw();
        device
            .d2d
            .Clear(Some(&primitives::color(Color::rgb(24, 48, 72))));
    }
    paint();
    unsafe {
        device.d2d.EndDraw(None, None).unwrap();
    }
    target.read_bgra(device).unwrap()
}

#[test]
#[ignore = "hardware Direct2D coverage proof; run alone without other GPU suites"]
fn native_gpu_coverage_matches_raster_pixels_and_reuses_geometry_across_colors() {
    let device = Device::hardware(None).unwrap();
    let target = Target::new(&device, 80, 80).unwrap();
    let mut coverage = Coverage::default();
    let bounds = D2D_RECT_F {
        left: 5.0,
        top: 7.0,
        right: 69.0,
        bottom: 71.0,
    };
    for radius in [0.0, 20.0] {
        for widths in [[8.0; 4], [3.0, 12.0, 5.0, 0.0], [2.0, 0.0, 0.0, 0.0]] {
            for colors in [
                [Color::rgb(255, 80, 20); 4],
                [
                    Color::rgb(255, 0, 0),
                    Color::rgb(0, 128, 0),
                    Color::rgb(0, 0, 255),
                    Color::TRANSPARENT,
                ],
            ] {
                let image = super::super::borders::raster(64, 64, widths, colors, radius).unwrap();
                let bitmap = device
                    .upload(image.width, image.height, &image.bgra)
                    .unwrap();
                let expected = render(&device, &target, || {
                    primitives::draw(
                        &device,
                        &bitmap,
                        D2D_RECT_F {
                            right: bounds.right + 1.0,
                            bottom: bounds.bottom + 1.0,
                            ..bounds
                        },
                    )
                });
                coverage.next_frame().unwrap();
                let actual = render(&device, &target, || {
                    let ring = ring(widths, radius);
                    let rect = Rect {
                        left: 0,
                        top: 0,
                        right: 64,
                        bottom: 64,
                    };
                    for (side, color) in colors.iter().enumerate() {
                        if color.alpha == 0 || widths[side] == 0.0 {
                            continue;
                        }
                        let key = format!("{radius}:{widths:?}:{side}");
                        assert!(
                            coverage
                                .paint(&device, &key, bounds, *color, || {
                                    let region = Region(unsafe {
                                        super::super::super::paint_primitives::border_side_region(
                                            &rect, widths, side, ring.0,
                                        )
                                    });
                                    assert!(!region.0.is_null());
                                    Ok(region)
                                })
                                .unwrap()
                        );
                    }
                });
                assert_eq!(
                    actual, expected,
                    "radius {radius}, widths {widths:?}, colors {colors:?}"
                );
            }
        }
    }
    let count = coverage.entries.len();
    render(&device, &target, || {
        assert!(
            coverage
                .paint(
                    &device,
                    "0:[8.0, 8.0, 8.0, 8.0]:0",
                    bounds,
                    Color::WHITE,
                    || panic!("cached geometry regenerated after a color change")
                )
                .unwrap()
        );
    });
    assert_eq!(coverage.entries.len(), count);
    let mut matrix = D2D1_LAYER_PARAMETERS1::default().maskTransform;
    matrix.M11 = 1.25;
    matrix.M22 = 1.25;
    unsafe {
        device.d2d.SetTransform(&matrix);
    }
    assert!(
        !coverage
            .paint(&device, "unsupported", bounds, Color::WHITE, || panic!(
                "non-integral geometry was admitted"
            ))
            .unwrap()
    );
    super::super::groups::identity(&device);
}
