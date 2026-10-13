//! Cache the existing native rounded-shape coverage, independent of animated Canvas pixels.
use super::*;
use better_web_browser::engine::css::Color;
use windows::Win32::Graphics::Gdi::{GetRegionData, HRGN, RGNDATAHEADER};

pub(super) use super::borders::paint as paint_border;

pub(super) fn paint(
    compositor: &mut Compositor,
    bounds: D2D_RECT_F,
    color: Color,
    radius: f32,
    border: Option<[f32; 4]>,
) -> Result<(), String> {
    let width = (bounds.right - bounds.left).max(0.0) as u32;
    let height = (bounds.bottom - bounds.top).max(0.0) as u32;
    if width == 0 || height == 0 || color.alpha == 0 {
        return Ok(());
    }
    if width > 7680
        || height > 4320
        || u64::from(width + 1) * u64::from(height + 1) * 4 > 64 * 1024 * 1024
    {
        return Err("native GPU shape exceeds the bitmap budget".into());
    }
    let widths = border.map(|values| values.map(|value| value.max(0.0).round()));
    let radius = radius.max(0.0);
    let key = format!(
        "native-shape:{width}:{height}:{}:{widths:?}:{color:?}",
        radius.to_bits()
    );
    // Opaque coverage can be filled directly; translucent shapes keep their
    // pre-multiplied raster bytes and existing source-over rounding.
    let coverage_key = format!(
        "native-coverage:{width}:{height}:{}:{widths:?}",
        radius.to_bits()
    );
    if color.alpha == 255
        && coverage::large_shape(width, height)
        && compositor
            .coverage
            .paint(&compositor.device, &coverage_key, bounds, color, || {
                native_region(width, height, widths, radius)
            })?
    {
        return Ok(());
    }
    let bitmap = compositor.images.generated(&compositor.device, &key, || {
        let region = native_region(width, height, widths, radius)?;
        pixels(region.0, width + 1, height + 1, color)
    })?;
    primitives::draw(
        &compositor.device,
        &bitmap,
        D2D_RECT_F {
            right: bounds.right + 1.0,
            bottom: bounds.bottom + 1.0,
            ..bounds
        },
    );
    Ok(())
}

fn native_region(
    width: u32,
    height: u32,
    widths: Option<[f32; 4]>,
    radius: f32,
) -> Result<coverage::Region, String> {
    let rect = Rect {
        left: 0,
        top: 0,
        right: width as i32,
        bottom: height as i32,
    };
    let region = coverage::Region(unsafe {
        if let Some(widths) = widths {
            super::super::paint_primitives::border_ring(&rect, widths, radius)
        } else {
            let diameter = (radius * 2.0).round().max(1.0) as i32;
            CreateRoundRectRgn(0, 0, rect.right + 1, rect.bottom + 1, diameter, diameter)
        }
    });
    if region.0.is_null() {
        Err("native GPU shape region is unavailable".into())
    } else {
        Ok(region)
    }
}

fn pixels(region: Hrgn, width: u32, height: u32, color: Color) -> Result<DecodedImage, String> {
    let mut bgra = vec![0; width as usize * height as usize * 4];
    fill_pixels(region, width, height, color, &mut bgra)?;
    Ok(DecodedImage {
        width,
        height,
        bgra: bgra.into(),
    })
}

pub(super) fn fill_pixels(
    region: Hrgn,
    width: u32,
    height: u32,
    color: Color,
    bgra: &mut [u8],
) -> Result<(), String> {
    let bytes = unsafe { GetRegionData(HRGN(region), 0, None) } as usize;
    if !(std::mem::size_of::<RGNDATAHEADER>()..=1024 * 1024).contains(&bytes) {
        return Err("native GPU shape region data exceeds limit".into());
    }
    // DWORD storage supplies the required alignment for RGNDATA and RECT.
    // https://learn.microsoft.com/windows/win32/api/wingdi/nf-wingdi-getregiondata
    let mut data = vec![0_u32; bytes.div_ceil(4)];
    let pointer = data.as_mut_ptr().cast();
    let written = unsafe { GetRegionData(HRGN(region), bytes as u32, Some(pointer)) } as usize;
    let header = unsafe { &*data.as_ptr().cast::<RGNDATAHEADER>() };
    let start = header.dwSize as usize;
    let count = header.nCount as usize;
    if written != bytes
        || start != std::mem::size_of::<RGNDATAHEADER>()
        || count > (bytes - start) / 16
    {
        return Err("native GPU shape has invalid region data".into());
    }
    let alpha = u16::from(color.alpha);
    let pixel = [
        ((u16::from(color.blue) * alpha + 127) / 255) as u8,
        ((u16::from(color.green) * alpha + 127) / 255) as u8,
        ((u16::from(color.red) * alpha + 127) / 255) as u8,
        color.alpha,
    ];
    for index in 0..count {
        let rect = unsafe {
            &*(data
                .as_ptr()
                .cast::<u8>()
                .add(start + index * 16)
                .cast::<windows::Win32::Foundation::RECT>())
        };
        for y in rect.top.max(0)..rect.bottom.min(height as i32) {
            for x in rect.left.max(0)..rect.right.min(width as i32) {
                let offset = (y as usize * width as usize + x as usize) * 4;
                bgra[offset..offset + 4].copy_from_slice(&pixel);
            }
        }
    }
    Ok(())
}
