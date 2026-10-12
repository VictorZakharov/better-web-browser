//! Native sampling into a zeroed, owned premultiplied DIB.
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) unsafe fn resample(
    source: Hbitmap,
    image: &DecodedImage,
    width: i32,
    height: i32,
    bytes: usize,
    dc: Hdc,
    reflection: Option<Reflection>,
) -> Option<Bitmap> {
    let mut info = bitmap_info(image);
    info.header.width = width;
    info.header.height = -height;
    info.header.size_image = bytes as u32;
    let mut bits = null_mut();
    let handle = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if handle.is_null() {
        return None;
    }
    let Some(pixels) = NonNull::new(bits.cast::<u8>()) else {
        DeleteObject(handle);
        return None;
    };
    let mut bitmap = Bitmap {
        handle,
        pixels,
        width: width as u32,
        height: height as u32,
        bytes,
        opaque: false,
        coverage: None,
    };
    // Zero preserves premultiplied colors and alpha. For reflection the native
    // matrix/placement is replayed exactly; pre-flipping texels is not equivalent.
    std::ptr::write_bytes(pixels.as_ptr(), 0, bytes);
    let source_dc = CreateCompatibleDC(dc);
    let target_dc = CreateCompatibleDC(dc);
    if source_dc.is_null() || target_dc.is_null() {
        if !source_dc.is_null() {
            DeleteDC(source_dc);
        }
        if !target_dc.is_null() {
            DeleteDC(target_dc);
        }
        return None;
    }
    let old_source = SelectObject(source_dc, source);
    let old_target = SelectObject(target_dc, handle);
    let selected = |previous: Hgdiobj| !previous.is_null() && previous as isize != -1;
    let destination = reflection.map_or(
        Some(Rect {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        }),
        |plan| plan.configure(target_dc),
    );
    let ok = selected(old_source)
        && selected(old_target)
        && destination.is_some_and(|rect| {
            AlphaBlend(
                target_dc,
                rect.left,
                rect.top,
                rect.width(),
                rect.height(),
                source_dc,
                0,
                0,
                image.width as i32,
                image.height as i32,
                BlendFunction {
                    operation: 0,
                    flags: 0,
                    source_constant_alpha: 255,
                    alpha_format: 1,
                },
            ) != 0
        })
        && GdiFlush() != 0;
    if selected(old_source) {
        SelectObject(source_dc, old_source);
    }
    if selected(old_target) {
        SelectObject(target_dc, old_target);
    }
    DeleteDC(source_dc);
    DeleteDC(target_dc);
    if !ok {
        return None;
    }
    // Coverage is derived from synchronized baked pixels, not the old source grid.
    let pixels = std::slice::from_raw_parts(bitmap.pixels.as_ptr(), bytes);
    bitmap.coverage = Some(super::super::coverage::derive_bytes(
        width as u32,
        height as u32,
        pixels,
    ));
    Some(bitmap)
}
