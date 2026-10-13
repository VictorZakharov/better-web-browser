//! Completed native draw commands with pixel verification outside the timing interval.
use super::*;
use std::time::{Duration, Instant};

struct Border {
    width: u32,
    height: u32,
    bounds: D2D_RECT_F,
    color: Color,
    key: String,
}
impl Border {
    fn region(&self) -> Region {
        let rect = Rect {
            left: 0,
            top: 0,
            right: self.width as i32,
            bottom: self.height as i32,
        };
        let region = Region(unsafe {
            super::super::super::super::paint_primitives::border_ring(&rect, [2.0; 4], 0.0)
        });
        assert!(!region.0.is_null());
        region
    }
    fn bitmap(&self, device: &Device) -> ID2D1Bitmap1 {
        let region = self.region();
        let mut pixels = vec![0; (self.width + 1) as usize * (self.height + 1) as usize * 4];
        super::super::super::shapes::fill_pixels(
            region.0,
            self.width + 1,
            self.height + 1,
            self.color,
            &mut pixels,
        )
        .unwrap();
        device
            .upload(self.width + 1, self.height + 1, &pixels)
            .unwrap()
    }
}
fn borders(width: u32, height: u32) -> Vec<Border> {
    [
        Color::WHITE,
        Color::rgb(240, 80, 20),
        Color::rgb(10, 180, 140),
        Color::rgb(80, 100, 230),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, color)| {
        let inset = 4 + index as u32 * 5;
        Border {
            width: width - inset * 2,
            height: height - inset * 2,
            color,
            key: index.to_string(),
            bounds: D2D_RECT_F {
                left: inset as f32,
                top: inset as f32,
                right: (width - inset) as f32,
                bottom: (height - inset) as f32,
            },
        }
    })
    .collect()
}
enum Route {
    Raster(Vec<ID2D1Bitmap1>),
    Geometry(Coverage),
}
impl Route {
    fn new(geometry: bool, device: &Device, borders: &[Border]) -> Self {
        if geometry {
            Self::Geometry(Coverage::default())
        } else {
            Self::Raster(borders.iter().map(|border| border.bitmap(device)).collect())
        }
    }
    fn draw(&mut self, device: &Device, borders: &[Border]) {
        match self {
            Self::Raster(bitmaps) => {
                for (border, bitmap) in borders.iter().zip(bitmaps) {
                    primitives::draw(
                        device,
                        bitmap,
                        D2D_RECT_F {
                            right: border.bounds.right + 1.0,
                            bottom: border.bounds.bottom + 1.0,
                            ..border.bounds
                        },
                    );
                }
            }
            Self::Geometry(coverage) => {
                coverage.next_frame().unwrap();
                for border in borders {
                    assert!(
                        coverage
                            .paint(device, &border.key, border.bounds, border.color, || Ok(
                                border.region()
                            ))
                            .unwrap()
                    );
                }
            }
        }
    }
}
fn draw(device: &Device, target: &Target, route: &mut Route, borders: &[Border]) {
    unsafe {
        device.d2d.SetTarget(&target.bitmap);
        device.d2d.SetAntialiasMode(D2D1_ANTIALIAS_MODE_ALIASED);
        super::super::super::groups::identity(device);
        device.d2d.BeginDraw();
        device
            .d2d
            .Clear(Some(&primitives::color(Color::rgb(24, 48, 72))));
    }
    route.draw(device, borders);
    unsafe {
        device.d2d.EndDraw(None, None).unwrap();
    }
    // Both routes include completion, not just queued submission. This wait is
    // confined to explicit command verification, never native window painting.
    let fence = Fence::submit(device).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !fence.complete(device).unwrap() {
        assert!(Instant::now() < deadline, "GPU draw completion timeout");
        std::thread::yield_now();
    }
}

#[test]
#[ignore = "release native border command timing; run alone with --nocapture"]
fn measure_completed_border_commands_with_exact_pixels() {
    let device = Device::hardware(None).unwrap();
    for (width, height) in [(128, 128), (1382, 760), (1920, 1080)] {
        let target = Target::new(&device, width, height).unwrap();
        let borders = borders(width, height);
        let mut reference = Route::new(false, &device, &borders);
        draw(&device, &target, &mut reference, &borders);
        let expected = target.read_bgra(&device).unwrap();
        drop(reference);
        for cold in [true, false] {
            for block in 0..4 {
                for geometry in if block % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                } {
                    let mut route = Some(Route::new(geometry, &device, &borders));
                    // Warm samples reuse native resources in both routes.
                    draw(&device, &target, route.as_mut().unwrap(), &borders);
                    assert_eq!(target.read_bgra(&device).unwrap(), expected);
                    for sample in 0..12 {
                        if cold {
                            route = None;
                        }
                        let started = Instant::now();
                        if cold {
                            route = Some(Route::new(geometry, &device, &borders));
                        }
                        draw(&device, &target, route.as_mut().unwrap(), &borders);
                        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
                        assert_eq!(
                            target.read_bgra(&device).unwrap(),
                            expected,
                            "{width}x{height}, cold={cold}, geometry={geometry}, block={block}, sample={sample}"
                        );
                        println!(
                            "border-command width={width} height={height} cold={cold} geometry={geometry} block={block} sample={sample} warmup={} ms={elapsed:.4}",
                            sample < 3
                        );
                    }
                }
            }
        }
        let raster_bytes: usize = borders
            .iter()
            .map(|border| (border.width + 1) as usize * (border.height + 1) as usize * 4)
            .sum();
        println!(
            "border-command-payload width={width} height={height} raster-bytes={raster_bytes} coverage-rectangles=16"
        );
    }
}
