//! Bake the actual horizontal native reflection, never a guessed flipped sampler.
use super::*;
use better_web_browser::engine::affine::Affine2D;
use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::Graphics::Gdi::{
    GM_ADVANCED, GetLayout, GetMapMode, GetWindowOrgEx, GetWorldTransform, MM_TEXT,
    SetGraphicsMode, SetWorldTransform, XFORM,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::windows_app) struct Reflection {
    local: [i32; 4],
    matrix: [u32; 6],
    bounds: [i32; 4],
}

impl Reflection {
    pub(in crate::windows_app) unsafe fn new(dc: Hdc, local: Rect) -> Option<Self> {
        let mut world: XFORM = std::mem::zeroed();
        let mut origin = POINT { x: 0, y: 0 };
        if GetWorldTransform(dc, &mut world) == 0
            || world.eM11 >= 0.0
            || world.eM22 <= 0.0
            || world.eM12 != 0.0
            || world.eM21 != 0.0
            || GetMapMode(dc) != MM_TEXT
            || GetLayout(dc) != 0
            || GetWindowOrgEx(dc, &mut origin) == 0
            || origin.x != 0
            || origin.y != 0
            || local.width() <= 0
            || local.height() <= 0
        {
            return None;
        }
        let matrix = [
            world.eM11, world.eM12, world.eM21, world.eM22, world.eDx, world.eDy,
        ];
        if !matrix.iter().all(|value| value.is_finite()) {
            return None;
        }
        let affine = Affine2D(matrix.map(f64::from));
        let (x0, y0) = affine.point(f64::from(local.left), f64::from(local.top));
        let (x1, y1) = affine.point(f64::from(local.right), f64::from(local.bottom));
        // Optional admission stays in a range where a one-pixel guard covers
        // native float rounding. Large coordinates retain direct painting.
        if [x0, y0, x1, y1]
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 1_048_576.0)
        {
            return None;
        }
        let bounds = [
            x1.floor() as i32 - 1,
            y0.floor() as i32 - 1,
            x0.ceil() as i32 + 1,
            y1.ceil() as i32 + 1,
        ];
        Some(Self {
            local: [local.left, local.top, local.right, local.bottom],
            matrix: matrix.map(f32::to_bits),
            bounds,
        })
    }

    pub(in crate::windows_app) fn bounds(self) -> Rect {
        Rect {
            left: self.bounds[0],
            top: self.bounds[1],
            right: self.bounds[2],
            bottom: self.bounds[3],
        }
    }

    pub(super) unsafe fn configure(self, dc: Hdc) -> Option<Rect> {
        let bounds = self.bounds();
        let matrix = self.matrix.map(f32::from_bits);
        let world = XFORM {
            eM11: matrix[0],
            eM12: matrix[1],
            eM21: matrix[2],
            eM22: matrix[3],
            eDx: matrix[4],
            eDy: matrix[5],
        };
        if SetViewportOrgEx(dc, -bounds.left, -bounds.top, null_mut()) == 0
            || SetGraphicsMode(dc, GM_ADVANCED) == 0
            || SetWorldTransform(dc, &world) == 0
        {
            return None;
        }
        Some(Rect {
            left: self.local[0],
            top: self.local[1],
            right: self.local[2],
            bottom: self.local[3],
        })
    }
}

impl Cache {
    pub(in crate::windows_app) unsafe fn reflected(
        &mut self,
        source_key: &str,
        image: &DecodedImage,
        source: Hbitmap,
        reflection: Reflection,
        dc: Hdc,
    ) -> Option<ScaledImage> {
        let bounds = reflection.bounds();
        let width = bounds.width();
        let height = bounds.height();
        if source.is_null()
            || !valid(image)
            || source_key.len() > MAX_SOURCE_KEY_BYTES
            || width <= 0
            || height <= 0
        {
            return None;
        }
        let bytes = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        if bytes < MIN_BYTES || bytes > self.byte_limit / 2 {
            return None;
        }
        if let Some(index) = self
            .entries
            .iter()
            .position(|v| v.key.matches(source_key, width, height, Some(reflection)))
        {
            let entry = self.entries.remove(index)?;
            let result = ScaledImage::from_bitmap(&entry.bitmap);
            self.entries.push_back(entry);
            return Some(result);
        }
        let index = self
            .pending
            .iter()
            .position(|v| v.matches(source_key, width, height, Some(reflection)));
        self.admit_pending(index, source_key, width, height, Some(reflection))?;
        self.make_room(bytes)?;
        let bitmap =
            unsafe { raster::resample(source, image, width, height, bytes, dc, Some(reflection)) }?;
        let result = ScaledImage::from_bitmap(&bitmap);
        self.bytes += bytes;
        self.entries.push_back(Variant {
            key: Key {
                source: source_key.into(),
                width,
                height,
                reflection: Some(reflection),
            },
            bitmap,
        });
        Some(result)
    }
}

#[cfg(test)]
mod tests;
