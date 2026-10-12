use super::*;
use crate::windows_app::paint_primitives::bitmap_info;

pub(super) struct Surface {
    pub dc: Hdc,
    pub bitmap: Hbitmap,
    previous: Hgdiobj,
    pixels: *mut u8,
    bytes: usize,
}

impl Surface {
    pub unsafe fn new(image: &DecodedImage) -> Self {
        let dc = CreateCompatibleDC(null_mut());
        assert!(!dc.is_null());
        let mut pixels = null_mut();
        let bitmap = CreateDIBSection(
            dc,
            &bitmap_info(image),
            DIB_RGB_COLORS,
            &mut pixels,
            null_mut(),
            0,
        );
        assert!(!bitmap.is_null() && !pixels.is_null());
        let previous = SelectObject(dc, bitmap);
        assert!(!previous.is_null() && previous as usize != usize::MAX);
        std::ptr::copy_nonoverlapping(image.bgra.as_ptr(), pixels.cast(), image.bgra.len());
        Self {
            dc,
            bitmap,
            previous,
            pixels: pixels.cast(),
            bytes: image.bgra.len(),
        }
    }

    pub unsafe fn bytes(&self) -> Vec<u8> {
        assert_ne!(GdiFlush(), 0);
        std::slice::from_raw_parts(self.pixels, self.bytes).to_vec()
    }

    pub unsafe fn detach(&self) {
        assert!(!SelectObject(self.dc, self.previous).is_null());
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            GdiFlush();
            SelectObject(self.dc, self.previous);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}

pub(super) fn image(width: u32, height: u32, opaque: bool) -> DecodedImage {
    let mut bytes = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            let alpha = if opaque {
                255
            } else {
                ((x * 7 + y * 11) % 256) as u8
            };
            let channel = |v: u32| ((v % 256) as u16 * u16::from(alpha) / 255) as u8;
            bytes.extend([channel(x * 17), channel(y * 23), channel(x + y * 3), alpha]);
        }
    }
    DecodedImage {
        width,
        height,
        bgra: bytes.into(),
    }
}
