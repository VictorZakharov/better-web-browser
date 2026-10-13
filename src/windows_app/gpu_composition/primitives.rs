use super::*;
use better_web_browser::engine::{RectF, css::Color};

pub(super) fn rect(value: RectF, scroll: i32, scale: f32) -> D2D_RECT_F {
    let rect = super::super::paint_primitives::screen_rect(value, scroll, 0, scale);
    D2D_RECT_F {
        left: rect.left as f32,
        top: rect.top as f32,
        right: rect.right as f32,
        bottom: rect.bottom as f32,
    }
}

pub(super) fn color(value: Color) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: f32::from(value.red) / 255.0,
        g: f32::from(value.green) / 255.0,
        b: f32::from(value.blue) / 255.0,
        a: f32::from(value.alpha) / 255.0,
    }
}

pub(super) fn image_rect(value: RectF, scroll: i32, scale: f32) -> D2D_RECT_F {
    let rect = super::super::paint_primitives::image_rect(
        value,
        (value.y * scale).round() as i32 - scroll,
        scale,
    );
    D2D_RECT_F {
        left: rect.left as f32,
        top: rect.top as f32,
        right: rect.right as f32,
        bottom: rect.bottom as f32,
    }
}

pub(super) fn solid(
    device: &Device,
    bounds: D2D_RECT_F,
    value: Color,
    radius: f32,
) -> Result<(), String> {
    if value.alpha == 0 || bounds.right <= bounds.left || bounds.bottom <= bounds.top {
        return Ok(());
    }
    let brush = unsafe { device.d2d.CreateSolidColorBrush(&color(value), None) }
        .map_err(|error| error.to_string())?;
    unsafe {
        if radius > 0.0 {
            let radius = radius
                .min((bounds.right - bounds.left) / 2.0)
                .min((bounds.bottom - bounds.top) / 2.0);
            device.d2d.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: bounds,
                    radiusX: radius,
                    radiusY: radius,
                },
                &brush,
            );
        } else {
            device.d2d.FillRectangle(&bounds, &brush);
        }
    }
    Ok(())
}

pub(super) fn draw(device: &Device, bitmap: &ID2D1Bitmap1, bounds: D2D_RECT_F) {
    if bounds.right <= bounds.left || bounds.bottom <= bounds.top {
        return;
    }
    unsafe {
        device.d2d.DrawBitmap(
            bitmap,
            Some(&bounds),
            1.0,
            D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
            None,
            None,
        );
    }
}
