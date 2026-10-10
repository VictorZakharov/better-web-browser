//! Pixel-checked GDI command timing, no window creation or screen painting.
//! Build with rustc -O; all output artifacts and temporary directories belong on G:.
use std::{ffi::c_void, ptr::null_mut, time::Instant};
type Handle = *mut c_void;
#[repr(C)]
struct Header {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bits: u16,
    compression: u32,
    image_size: u32,
    x_pixels: i32,
    y_pixels: i32,
    colors: u32,
    important: u32,
}
#[repr(C)]
struct Info {
    header: Header,
    color: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Blend {
    operation: u8,
    flags: u8,
    alpha: u8,
    format: u8,
}
#[link(name = "user32")]
unsafe extern "system" {
    fn GetDC(window: Handle) -> Handle;
    fn ReleaseDC(window: Handle, dc: Handle) -> i32;
}
#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateCompatibleDC(dc: Handle) -> Handle;
    fn CreateDIBSection(
        dc: Handle,
        info: *const Info,
        usage: u32,
        pixels: *mut Handle,
        section: Handle,
        offset: u32,
    ) -> Handle;
    fn SelectObject(dc: Handle, object: Handle) -> Handle;
    fn DeleteObject(object: Handle) -> i32;
    fn DeleteDC(dc: Handle) -> i32;
    fn GdiFlush() -> i32;
    fn SetStretchBltMode(dc: Handle, mode: i32) -> i32;
    fn StretchBlt(
        dc: Handle,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        source: Handle,
        sx: i32,
        sy: i32,
        sw: i32,
        sh: i32,
        operation: u32,
    ) -> i32;
    fn IntersectClipRect(dc: Handle, left: i32, top: i32, right: i32, bottom: i32) -> i32;
    fn SaveDC(dc: Handle) -> i32;
    fn RestoreDC(dc: Handle, saved: i32) -> i32;
}
#[link(name = "msimg32")]
unsafe extern "system" {
    fn AlphaBlend(
        dc: Handle,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        source: Handle,
        sx: i32,
        sy: i32,
        sw: i32,
        sh: i32,
        blend: Blend,
    ) -> i32;
}
struct Surface {
    dc: Handle,
    bitmap: Handle,
    previous: Handle,
    pixels: *mut u8,
    bytes: usize,
}
impl Surface {
    unsafe fn new(reference: Handle, width: i32, height: i32) -> Self {
        let dc = unsafe { CreateCompatibleDC(reference) };
        assert!(!dc.is_null());
        let info = Info {
            header: Header {
                size: 40,
                width,
                height: -height,
                planes: 1,
                bits: 32,
                compression: 0,
                image_size: 0,
                x_pixels: 0,
                y_pixels: 0,
                colors: 0,
                important: 0,
            },
            color: 0,
        };
        let mut pixels = null_mut();
        let bitmap = unsafe { CreateDIBSection(reference, &info, 0, &mut pixels, null_mut(), 0) };
        assert!(!bitmap.is_null() && !pixels.is_null());
        let previous = unsafe { SelectObject(dc, bitmap) };
        Self {
            dc,
            bitmap,
            previous,
            pixels: pixels.cast(),
            bytes: width as usize * height as usize * 4,
        }
    }
    unsafe fn reset(&self) {
        assert_ne!(unsafe { GdiFlush() }, 0);
        unsafe {
            std::ptr::write_bytes(self.pixels, 77, self.bytes);
        }
    }
    unsafe fn bytes(&self) -> &[u8] {
        assert_ne!(unsafe { GdiFlush() }, 0);
        unsafe { std::slice::from_raw_parts(self.pixels, self.bytes) }
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            GdiFlush();
            SelectObject(self.dc, self.previous);
            assert_ne!(DeleteObject(self.bitmap), 0);
            assert_ne!(DeleteDC(self.dc), 0);
        }
    }
}
unsafe fn draw(destination: &Surface, source: &Surface, rect: [i32; 4], opaque: bool) {
    let [x, y, w, h] = rect;
    let result = if opaque {
        let previous = unsafe { SetStretchBltMode(destination.dc, 3) };
        assert_ne!(previous, 0);
        let result = unsafe {
            StretchBlt(
                destination.dc,
                x,
                y,
                w,
                h,
                source.dc,
                0,
                0,
                1536,
                658,
                0x00cc0020,
            )
        };
        unsafe {
            SetStretchBltMode(destination.dc, previous);
        }
        result
    } else {
        unsafe {
            AlphaBlend(
                destination.dc,
                x,
                y,
                w,
                h,
                source.dc,
                0,
                0,
                1536,
                658,
                Blend {
                    operation: 0,
                    flags: 0,
                    alpha: 255,
                    format: 1,
                },
            )
        }
    };
    assert_ne!(result, 0);
}
fn main() {
    unsafe {
        // The display DC establishes format only. Every draw targets an owned memory DC.
        let reference = GetDC(null_mut());
        assert!(!reference.is_null());
        let source = Surface::new(reference, 1536, 658);
        let bytes = std::slice::from_raw_parts_mut(source.pixels, source.bytes);
        for (index, pixel) in bytes.chunks_exact_mut(4).enumerate() {
            pixel.copy_from_slice(&[
                (index % 251) as u8,
                (index / 47 % 253) as u8,
                (index / 313 % 255) as u8,
                255,
            ]);
        }
        let alpha = Surface::new(reference, 1920, 824);
        let copy = Surface::new(reference, 1920, 824);
        for rect in [
            [0, 0, 1536, 658],
            [0, 0, 1920, 824],
            [21, 17, 960, 412],
            [-13, -11, 1917, 823],
        ] {
            for clipped in [false, true] {
                alpha.reset();
                copy.reset();
                let sa = SaveDC(alpha.dc);
                let sc = SaveDC(copy.dc);
                if clipped {
                    IntersectClipRect(alpha.dc, 29, 31, 1787, 733);
                    IntersectClipRect(copy.dc, 29, 31, 1787, 733);
                }
                draw(&alpha, &source, rect, false);
                draw(&copy, &source, rect, true);
                let a = alpha.bytes();
                let c = copy.bytes();
                let rgb_differences = a
                    .chunks_exact(4)
                    .zip(c.chunks_exact(4))
                    .filter(|(a, c)| a[..3] != c[..3])
                    .count();
                let alpha_differences = a
                    .chunks_exact(4)
                    .zip(c.chunks_exact(4))
                    .filter(|(a, c)| a[3] != c[3])
                    .count();
                println!("pixels rect={rect:?} clip={clipped} RGB differences={rgb_differences}, alpha differences={alpha_differences}");
                assert_eq!(rgb_differences, 0);
                assert_eq!(alpha_differences, 0);
                RestoreDC(alpha.dc, sa);
                RestoreDC(copy.dc, sc);
            }
        }
        for rotation in 0..3 {
            for opaque in if rotation % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let mut samples = Vec::new();
                for _ in 0..25 {
                    let started = Instant::now();
                    draw(
                        if opaque { &copy } else { &alpha },
                        &source,
                        [0, 0, 1920, 824],
                        opaque,
                    );
                    assert_ne!(GdiFlush(), 0);
                    samples.push(started.elapsed().as_secs_f64() * 1000.0);
                }
                println!(
                    "rotation={rotation} {} samples_ms={samples:?}",
                    if opaque { "opaque-copy" } else { "alpha-blend" }
                );
            }
        }
        // A sparse border image: no nonzero premultiplied channels in the interior.
        // Four disjoint coarse strips preserve even partially transparent edge pixels.
        assert_ne!(GdiFlush(), 0);
        let bytes = std::slice::from_raw_parts_mut(source.pixels, source.bytes);
        for (index, pixel) in bytes.chunks_exact_mut(4).enumerate() {
            let x = index % 1536;
            let y = index / 1536;
            pixel.copy_from_slice(if x < 4 || x >= 1532 || y < 4 || y >= 654 {
                &[31, 63, 96, 128]
            } else {
                &[0, 0, 0, 0]
            });
        }
        for origin in [[0, 0], [21, 17], [-13, -11]] {
            for clipped in [false, true] {
                alpha.reset();
                copy.reset();
                let sa = SaveDC(alpha.dc);
                let sc = SaveDC(copy.dc);
                if clipped {
                    IntersectClipRect(alpha.dc, 29, 31, 1787, 733);
                    IntersectClipRect(copy.dc, 29, 31, 1787, 733);
                }
                draw(&alpha, &source, [origin[0], origin[1], 1536, 658], false);
                sparse(&copy, &source, origin);
                assert_eq!(
                    alpha.bytes(),
                    copy.bytes(),
                    "sparse origin={origin:?} clip={clipped}"
                );
                RestoreDC(alpha.dc, sa);
                RestoreDC(copy.dc, sc);
            }
        }
        println!(
            "sparse border: every BGRA byte matches at three translations, with/without clipping"
        );
        for rotation in 0..3 {
            for parts in if rotation % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let mut samples = Vec::new();
                for _ in 0..25 {
                    let start = Instant::now();
                    if parts {
                        sparse(&copy, &source, [0, 0]);
                    } else {
                        draw(&alpha, &source, [0, 0, 1536, 658], false);
                    }
                    assert_ne!(GdiFlush(), 0);
                    samples.push(start.elapsed().as_secs_f64() * 1000.0);
                }
                println!("sparse rotation={rotation} parts={parts} samples_ms={samples:?}");
            }
        }
        drop(copy);
        drop(alpha);
        drop(source);
        assert_ne!(ReleaseDC(null_mut(), reference), 0);
    }
}

unsafe fn sparse(destination: &Surface, source: &Surface, origin: [i32; 2]) {
    for [x, y, w, h] in [
        [0, 0, 1536, 64],
        [0, 594, 1536, 64],
        [0, 64, 64, 530],
        [1472, 64, 64, 530],
    ] {
        assert_ne!(
            unsafe {
                AlphaBlend(
                    destination.dc,
                    origin[0] + x,
                    origin[1] + y,
                    w,
                    h,
                    source.dc,
                    x,
                    y,
                    w,
                    h,
                    Blend {
                        operation: 0,
                        flags: 0,
                        alpha: 255,
                        format: 1,
                    },
                )
            },
            0
        );
    }
}
