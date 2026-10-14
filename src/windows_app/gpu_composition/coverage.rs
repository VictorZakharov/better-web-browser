//! Native border regions retain coverage geometry instead of transparent box-sized textures.
use super::*;
use better_web_browser::engine::css::Color;
use std::collections::HashMap;
use windows::Win32::Graphics::Gdi::{GetRegionData, HRGN, RGNDATAHEADER};
use windows_numerics::Vector2;

const MAX_RECTANGLES: usize = 65_536;
const MAX_ENTRIES: usize = 4096;

pub(super) fn large_shape(width: u32, height: u32) -> bool {
    // The completed-command workload favors cached bitmaps for small redraws.
    // Keep their cheap draw route; large empty interiors dominate cold allocation.
    u64::from(width) * u64::from(height) >= 16_384
}

pub(super) struct Region(pub(super) Hrgn);
impl Drop for Region {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.0);
        }
    }
}

pub(super) fn rectangles(region: Hrgn) -> Result<Vec<Rect>, String> {
    let bytes = unsafe { GetRegionData(HRGN(region), 0, None) } as usize;
    if !(std::mem::size_of::<RGNDATAHEADER>()..=1024 * 1024).contains(&bytes) {
        return Err("native GPU coverage exceeds the region data limit".into());
    }
    // DWORD storage supplies RGNDATA/RECT alignment. GetRegionData returns
    // disjoint scan rectangles, preserving the existing GDI corner ownership.
    // https://learn.microsoft.com/windows/win32/api/wingdi/nf-wingdi-getregiondata
    let mut data = vec![0_u32; bytes.div_ceil(4)];
    let written =
        unsafe { GetRegionData(HRGN(region), bytes as u32, Some(data.as_mut_ptr().cast())) }
            as usize;
    let header = unsafe { &*data.as_ptr().cast::<RGNDATAHEADER>() };
    let start = header.dwSize as usize;
    let count = header.nCount as usize;
    if written != bytes
        || start != std::mem::size_of::<RGNDATAHEADER>()
        || header.iType != 1
        || count > (bytes - start) / 16
    {
        return Err("native GPU coverage has invalid region data".into());
    }
    let mut output = Vec::with_capacity(count);
    for index in 0..count {
        let rect = unsafe {
            &*data
                .as_ptr()
                .cast::<u8>()
                .add(start + index * 16)
                .cast::<windows::Win32::Foundation::RECT>()
        };
        output.push(Rect {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        });
    }
    Ok(output)
}

struct Entry {
    geometry: ID2D1PathGeometry,
    rectangles: usize,
    used: u64,
}
#[derive(Default)]
pub(super) struct Coverage {
    entries: HashMap<String, Entry>,
    rectangles: usize,
    epoch: u64,
}
impl Coverage {
    pub(super) fn report(&self) -> serde_json::Value {
        serde_json::json!({"rectangles":self.rectangles,"rectangle_limit":MAX_RECTANGLES,
            "entries":self.entries.len(),"entry_limit":MAX_ENTRIES,
            "scope":"cached source coverage complexity; excludes native geometry overhead"})
    }
    pub(super) fn next_frame(&mut self) -> Result<(), String> {
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or("GPU coverage epoch exhausted")?;
        Ok(())
    }

    pub(super) fn paint(
        &mut self,
        device: &Device,
        key: &str,
        bounds: D2D_RECT_F,
        color: Color,
        generate: impl FnOnce() -> Result<Region, String>,
    ) -> Result<bool, String> {
        if color.alpha == 0 {
            return Ok(true);
        }
        if color.alpha != 255 {
            return Ok(false);
        }
        let mut previous = D2D1_LAYER_PARAMETERS1::default().maskTransform;
        unsafe {
            device.d2d.GetTransform(&mut previous);
        }
        // Geometry and nearest-neighbor bitmap edges agree at integral device
        // coordinates. Keep the existing raster route for every other transform.
        if previous.M11 != 1.0
            || previous.M12 != 0.0
            || previous.M21 != 0.0
            || previous.M22 != 1.0
            || previous.M31.fract() != 0.0
            || previous.M32.fract() != 0.0
            || bounds.left.fract() != 0.0
            || bounds.top.fract() != 0.0
            || bounds.right.fract() != 0.0
            || bounds.bottom.fract() != 0.0
        {
            return Ok(false);
        }
        let geometry = if let Some(entry) = self.entries.get_mut(key) {
            entry.used = self.epoch;
            entry.geometry.clone()
        } else {
            let region = generate()?;
            let scans = rectangles(region.0)?;
            if scans.len() > MAX_RECTANGLES {
                return Ok(false);
            }
            while self.rectangles.saturating_add(scans.len()) > MAX_RECTANGLES
                || self.entries.len() >= MAX_ENTRIES
            {
                let Some(old) = self
                    .entries
                    .iter()
                    .filter(|(_, entry)| entry.used != self.epoch)
                    .min_by_key(|(_, entry)| entry.used)
                    .map(|(key, _)| key.clone())
                else {
                    return Ok(false);
                };
                if let Some(entry) = self.entries.remove(&old) {
                    self.rectangles -= entry.rectangles;
                }
            }
            let geometry = path(device, &scans)?;
            self.rectangles += scans.len();
            self.entries.insert(
                key.to_owned(),
                Entry {
                    geometry: geometry.clone(),
                    rectangles: scans.len(),
                    used: self.epoch,
                },
            );
            geometry
        };
        let brush = unsafe {
            device
                .d2d
                .CreateSolidColorBrush(&primitives::color(color), None)
        }
        .map_err(|error| error.to_string())?;
        let mut translated = previous;
        translated.M31 += bounds.left;
        translated.M32 += bounds.top;
        unsafe {
            device.d2d.SetTransform(&translated);
            device.d2d.FillGeometry(&geometry, &brush, None);
            device.d2d.SetTransform(&previous);
        }
        Ok(true)
    }
}

fn path(device: &Device, scans: &[Rect]) -> Result<ID2D1PathGeometry, String> {
    let geometry = unsafe {
        device
            .d2d
            .GetFactory()
            .and_then(|factory| factory.CreatePathGeometry())
    }
    .map_err(|error| error.to_string())?;
    let sink = unsafe { geometry.Open() }.map_err(|error| error.to_string())?;
    unsafe {
        sink.SetFillMode(D2D1_FILL_MODE_WINDING);
        for rect in scans {
            sink.BeginFigure(
                Vector2 {
                    X: rect.left as f32,
                    Y: rect.top as f32,
                },
                D2D1_FIGURE_BEGIN_FILLED,
            );
            sink.AddLines(&[
                Vector2 {
                    X: rect.right as f32,
                    Y: rect.top as f32,
                },
                Vector2 {
                    X: rect.right as f32,
                    Y: rect.bottom as f32,
                },
                Vector2 {
                    X: rect.left as f32,
                    Y: rect.bottom as f32,
                },
            ]);
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        }
        sink.Close().map_err(|error| error.to_string())?;
    }
    Ok(geometry)
}

#[cfg(test)]
mod tests;
