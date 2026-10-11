//! Native affine state applies to the existing painter, including premultiplied images.
//! Source DIBs remain untransformed during copies; only drawing uses their borrowed matrix.
use super::super::platform::*;
use better_web_browser::engine::{RectF, affine::Affine2D};
use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::Graphics::Gdi::{
    CreatePolygonRgn, ExtSelectClipRgn, GM_ADVANCED, GetViewportOrgEx, GetWorldTransform, RGN_AND,
    SetGraphicsMode, SetWorldTransform, WINDING, XFORM,
};
mod device_images;
pub(in crate::windows_app) use device_images::ScaledDestination;

pub(super) unsafe fn matrix(dc: Hdc) -> Affine2D {
    let mut v = XFORM {
        eM11: 1.0,
        eM12: 0.0,
        eM21: 0.0,
        eM22: 1.0,
        eDx: 0.0,
        eDy: 0.0,
    };
    GetWorldTransform(dc, &mut v);
    Affine2D([v.eM11, v.eM12, v.eM21, v.eM22, v.eDx, v.eDy].map(f64::from))
}

pub(super) unsafe fn set(dc: Hdc, matrix: Affine2D) -> bool {
    let values = matrix.0.map(|v| v as f32);
    if !values.iter().all(|v| v.is_finite()) {
        return false;
    }
    let v = XFORM {
        eM11: values[0],
        eM12: values[1],
        eM21: values[2],
        eM22: values[3],
        eDx: values[4],
        eDy: values[5],
    };
    SetGraphicsMode(dc, GM_ADVANCED) != 0 && SetWorldTransform(dc, &v) != 0
}

pub(super) fn screen_matrix(matrix: Affine2D, scroll_y: i32, toolbar: i32, scale: f32) -> Affine2D {
    let offset = f64::from(toolbar) - f64::from(scroll_y);
    let scale = f64::from(scale);
    let screen = Affine2D([scale, 0.0, 0.0, scale, 0.0, offset]);
    screen.then_local(matrix).then_local(Affine2D([
        1.0 / scale,
        0.0,
        0.0,
        1.0 / scale,
        0.0,
        -offset / scale,
    ]))
}

pub(super) unsafe fn device_bounds(dc: Hdc, rect: Rect) -> Rect {
    bounds(matrix(dc), rect)
}

pub(super) fn bounds(matrix: Affine2D, rect: Rect) -> Rect {
    let transformed = matrix.bounds(RectF {
        x: rect.left as f32,
        y: rect.top as f32,
        width: rect.width() as f32,
        height: rect.height() as f32,
    });
    Rect {
        left: transformed.x.floor() as i32,
        top: transformed.y.floor() as i32,
        right: transformed.right().ceil() as i32,
        bottom: transformed.bottom().ceil() as i32,
    }
}

pub(super) unsafe fn local_content(dc: Hdc, rect: Rect) -> Rect {
    matrix(dc)
        .inverse()
        .map_or(Rect::default(), |inverse| bounds(inverse, rect))
}

/// Region APIs consume device coordinates, including a borrowed layer's viewport offset.
pub(in crate::windows_app) unsafe fn clip_rect(dc: Hdc, rect: Rect) -> bool {
    let matrix = matrix(dc);
    if matrix.translation().is_some() {
        return IntersectClipRect(dc, rect.left, rect.top, rect.right, rect.bottom) != 0;
    }
    let mut viewport = POINT { x: 0, y: 0 };
    if GetViewportOrgEx(dc, &mut viewport) == 0 {
        return false;
    }
    let points = [
        (rect.left, rect.top),
        (rect.right, rect.top),
        (rect.right, rect.bottom),
        (rect.left, rect.bottom),
    ]
    .map(|(x, y)| {
        let (x, y) = matrix.point(f64::from(x), f64::from(y));
        POINT {
            x: (x.round() as i32).saturating_add(viewport.x),
            y: (y.round() as i32).saturating_add(viewport.y),
        }
    });
    let region = CreatePolygonRgn(points.as_ptr(), 4, WINDING);
    if region.is_null() {
        return false;
    }
    let ok = ExtSelectClipRgn(dc, region, RGN_AND) != 0;
    DeleteObject(region);
    ok
}

pub(super) struct IdentityScope {
    dc: Hdc,
    saved: i32,
}
impl IdentityScope {
    pub(super) unsafe fn new(dc: Hdc) -> Option<Self> {
        let saved = SaveDC(dc);
        if saved == 0 {
            return None;
        }
        if !set(dc, Affine2D::IDENTITY) {
            RestoreDC(dc, saved);
            return None;
        }
        Some(Self { dc, saved })
    }
}
impl Drop for IdentityScope {
    fn drop(&mut self) {
        unsafe {
            RestoreDC(self.dc, self.saved);
        }
    }
}

#[cfg(test)]
mod tests;
