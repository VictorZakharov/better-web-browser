//! Windowless native pixel contracts. No browser or visible window is launched.
use super::*;
use crate::windows_app::paint_primitives::{bitmap_info, fill_color_rect};
use crate::windows_app::painting::groups::PaintGroups;
use better_web_browser::engine::{DecodedImage, DisplayItem};
use std::{collections::HashMap, ptr::null_mut};

struct Surface {
    dc: Hdc,
    bitmap: Hbitmap,
    previous: Hgdiobj,
    pixels: *mut u8,
    width: usize,
    height: usize,
}
impl Surface {
    unsafe fn new(width: usize, height: usize) -> Self {
        let image = DecodedImage {
            width: width as u32,
            height: height as u32,
            bgra: vec![0; width * height * 4].into(),
        };
        let dc = CreateCompatibleDC(null_mut());
        assert!(!dc.is_null());
        let mut pixels = null_mut();
        let bitmap = CreateDIBSection(dc, &bitmap_info(&image), 0, &mut pixels, null_mut(), 0);
        assert!(!bitmap.is_null() && !pixels.is_null());
        let previous = SelectObject(dc, bitmap);
        let result = Self {
            dc,
            bitmap,
            previous,
            pixels: pixels.cast(),
            width,
            height,
        };
        fill_color_rect(dc, &result.rect(), 0xff0000);
        result
    }
    fn rect(&self) -> Rect {
        Rect {
            left: 0,
            top: 0,
            right: self.width as i32,
            bottom: self.height as i32,
        }
    }
    unsafe fn rgb(&self, x: usize, y: usize) -> [u8; 3] {
        assert!(x < self.width && y < self.height);
        GdiFlush();
        let pixels = std::slice::from_raw_parts(self.pixels, self.width * self.height * 4);
        let i = (y * self.width + x) * 4;
        [pixels[i + 2], pixels[i + 1], pixels[i]]
    }
    unsafe fn handle(
        &self,
        groups: &mut PaintGroups,
        item: &DisplayItem,
        images: &HashMap<String, DecodedImage>,
    ) -> bool {
        groups.handle(item, self.dc, images, 0, 0, 1.0, &self.rect(), &self.rect())
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.previous);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}
fn bounds() -> RectF {
    RectF {
        x: 2.0,
        y: 2.0,
        width: 100.0,
        height: 8.0,
    }
}

#[test]
fn native_progress_fill_draws_only_its_scaled_prefix() {
    unsafe {
        for p in [0.04, 0.25, 0.5, 1.0] {
            let surface = Surface::new(110, 16);
            let mut groups = PaintGroups::default();
            let images = HashMap::new();
            let matrix = Affine2D::translate(2.0, 2.0)
                .then_local(Affine2D([p, 0.0, 0.0, 1.0, 0.0, 0.0]))
                .then_local(Affine2D::translate(-2.0, -2.0));
            surface.handle(
                &mut groups,
                &DisplayItem::BeginTransform {
                    bounds: matrix.bounds(bounds()),
                    matrix,
                },
                &images,
            );
            fill_color_rect(
                groups.dc(surface.dc),
                &Rect {
                    left: 2,
                    top: 2,
                    right: 102,
                    bottom: 10,
                },
                0x0000ff,
            );
            surface.handle(
                &mut groups,
                &DisplayItem::EndTransform {
                    bounds: matrix.bounds(bounds()),
                },
                &images,
            );
            let end = 2 + (100.0 * p).round() as usize;
            assert_eq!(surface.rgb(2, 5), [255, 0, 0], "p={p}");
            assert_eq!(surface.rgb(end - 1, 5), [255, 0, 0], "p={p}");
            assert_eq!(surface.rgb(end, 5), [0, 0, 255], "p={p}");
            assert_eq!(super::matrix(surface.dc), Affine2D::IDENTITY);
        }
    }
}

#[test]
fn native_transformed_opacity_uses_the_real_backdrop_in_both_nesting_orders() {
    unsafe {
        for transform_outer in [true, false] {
            let surface = Surface::new(32, 16);
            let mut groups = PaintGroups::default();
            let images = HashMap::new();
            let bounds = RectF {
                x: 2.0,
                y: 2.0,
                width: 8.0,
                height: 4.0,
            };
            let matrix = Affine2D([2.0, 0.0, 0.0, 2.0, 0.0, 0.0]);
            let transform = DisplayItem::BeginTransform {
                bounds: matrix.bounds(bounds),
                matrix,
            };
            let opacity = DisplayItem::BeginOpacity {
                bounds: if transform_outer {
                    bounds
                } else {
                    matrix.bounds(bounds)
                },
                opacity: 0.5,
            };
            let starts = if transform_outer {
                [&transform, &opacity]
            } else {
                [&opacity, &transform]
            };
            for item in starts {
                surface.handle(&mut groups, item, &images);
            }
            fill_color_rect(
                groups.dc(surface.dc),
                &Rect {
                    left: 2,
                    top: 2,
                    right: 10,
                    bottom: 6,
                },
                0x0000ff,
            );
            let end_transform = DisplayItem::EndTransform {
                bounds: matrix.bounds(bounds),
            };
            let end_opacity = DisplayItem::EndOpacity {
                bounds: if transform_outer {
                    bounds
                } else {
                    matrix.bounds(bounds)
                },
            };
            let ends = if transform_outer {
                [&end_opacity, &end_transform]
            } else {
                [&end_transform, &end_opacity]
            };
            for item in ends {
                surface.handle(&mut groups, item, &images);
            }
            let [red, green, blue] = surface.rgb(8, 8);
            assert!(
                (127..=129).contains(&red) && green == 0 && (126..=128).contains(&blue),
                "order={transform_outer}: {red},{green},{blue}"
            );
            assert_eq!(surface.rgb(3, 8), [0, 0, 255]);
        }
    }
}

#[test]
fn rotated_clip_rejects_bounding_box_corners_and_restores_the_dc() {
    unsafe {
        let surface = Surface::new(40, 40);
        let mut groups = PaintGroups::default();
        let images = HashMap::new();
        let a = std::f64::consts::FRAC_1_SQRT_2;
        let matrix = Affine2D([a, a, -a, a, 20.0, 4.0]);
        let bounds = RectF {
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 20.0,
        };
        surface.handle(
            &mut groups,
            &DisplayItem::BeginTransform {
                bounds: matrix.bounds(bounds),
                matrix,
            },
            &images,
        );
        let saved = SaveDC(surface.dc);
        assert!(clip_rect(
            surface.dc,
            Rect {
                left: 0,
                top: 0,
                right: 20,
                bottom: 20
            }
        ));
        fill_color_rect(
            surface.dc,
            &Rect {
                left: -10,
                top: -10,
                right: 30,
                bottom: 30,
            },
            0x0000ff,
        );
        RestoreDC(surface.dc, saved);
        surface.handle(
            &mut groups,
            &DisplayItem::EndTransform {
                bounds: matrix.bounds(bounds),
            },
            &images,
        );
        assert_eq!(surface.rgb(20, 15), [255, 0, 0]);
        assert_eq!(surface.rgb(7, 5), [0, 0, 255]);
        assert_eq!(super::matrix(surface.dc), Affine2D::IDENTITY);
    }
}

#[test]
fn rotated_mask_alpha_is_sampled_in_local_coordinates_over_the_real_backdrop() {
    unsafe {
        let surface = Surface::new(24, 20);
        let mut groups = PaintGroups::default();
        let images = HashMap::from([(
            "mask".into(),
            DecodedImage {
                width: 2,
                height: 1,
                bgra: vec![255, 255, 255, 255, 0, 0, 0, 0].into(),
            },
        )]);
        let bounds = RectF {
            x: 2.0,
            y: 2.0,
            width: 8.0,
            height: 4.0,
        };
        let matrix = Affine2D([0.0, 1.0, -1.0, 0.0, 12.0, 0.0]);
        surface.handle(
            &mut groups,
            &DisplayItem::BeginTransform {
                bounds: matrix.bounds(bounds),
                matrix,
            },
            &images,
        );
        surface.handle(
            &mut groups,
            &DisplayItem::BeginMask {
                bounds,
                url: "mask".into(),
            },
            &images,
        );
        fill_color_rect(
            groups.dc(surface.dc),
            &Rect {
                left: 2,
                top: 2,
                right: 10,
                bottom: 6,
            },
            0x0000ff,
        );
        surface.handle(&mut groups, &DisplayItem::EndMask { bounds }, &images);
        surface.handle(
            &mut groups,
            &DisplayItem::EndTransform {
                bounds: matrix.bounds(bounds),
            },
            &images,
        );
        assert_eq!(surface.rgb(8, 3), [255, 0, 0]);
        assert_eq!(surface.rgb(8, 8), [0, 0, 255]);
        assert_eq!(surface.rgb(3, 8), [0, 0, 255]);
    }
}

#[test]
fn transparent_images_follow_the_affine_destination_without_double_alpha() {
    unsafe {
        let surface = Surface::new(24, 20);
        let mut groups = PaintGroups::default();
        let images = HashMap::from([(
            "image".into(),
            DecodedImage {
                width: 2,
                height: 1,
                bgra: vec![0, 0, 128, 128, 0, 0, 0, 0].into(),
            },
        )]);
        let local = RectF {
            x: 2.0,
            y: 2.0,
            width: 8.0,
            height: 4.0,
        };
        let matrix = Affine2D([0.0, 1.0, -1.0, 0.0, 12.0, 0.0]);
        let bounds = matrix.bounds(local);
        surface.handle(
            &mut groups,
            &DisplayItem::BeginTransform { bounds, matrix },
            &images,
        );
        let item = DisplayItem::Image {
            rect: local,
            clip: None,
            url: "image".into(),
            alt: String::new(),
            tint: None,
        };
        let mut bitmaps = crate::windows_app::rendering_resources::ImageBitmaps::default();
        super::super::images::paint(
            &item,
            groups.dc(surface.dc),
            null_mut(),
            &images,
            &mut bitmaps,
            0,
            0,
            1.0,
            &surface.rect(),
            None,
        );
        surface.handle(&mut groups, &DisplayItem::EndTransform { bounds }, &images);
        let [red, green, blue] = surface.rgb(8, 3);
        assert!(
            (127..=129).contains(&red) && green == 0 && (126..=128).contains(&blue),
            "{red},{green},{blue}"
        );
        assert_eq!(surface.rgb(8, 9), [0, 0, 255]);
    }
}
