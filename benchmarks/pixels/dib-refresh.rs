//! Isolated Windows bitmap-admission workload; no window or browser is created.
//! Reallocation/reference and synchronized reuse copy identical changed bytes.
use std::ffi::c_void;
use std::ptr::{NonNull, null_mut};
use std::time::Instant;

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

#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateDIBSection(
        dc: Handle,
        info: *const Info,
        usage: u32,
        pixels: *mut Handle,
        section: Handle,
        offset: u32,
    ) -> Handle;
    fn DeleteObject(object: Handle) -> i32;
    fn GdiFlush() -> i32;
}

struct Bitmap {
    handle: NonNull<c_void>,
    pixels: NonNull<u8>,
    length: usize,
}

impl Bitmap {
    fn new(width: u32, height: u32, bytes: &[u8]) -> Self {
        assert!(width > 0 && height > 0);
        assert_eq!(bytes.len(), width as usize * height as usize * 4);
        let info = Info {
            header: Header {
                size: 40,
                width: width as i32,
                height: -(height as i32),
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
        // SAFETY: exact Win32 ABI/layout, no palette, mapping, window or DC.
        let handle = unsafe { CreateDIBSection(null_mut(), &info, 0, &mut pixels, null_mut(), 0) };
        let bitmap = Self {
            handle: NonNull::new(handle).expect("CreateDIBSection"),
            pixels: NonNull::new(pixels.cast()).expect("DIB storage"),
            length: bytes.len(),
        };
        bitmap.refresh(bytes);
        bitmap
    }

    fn refresh(&self, bytes: &[u8]) {
        assert_eq!(bytes.len(), self.length);
        // Win32 requires flushing this thread's GDI batch before CPU writes.
        // Even this no-drawing workload includes that synchronization cost.
        assert_ne!(unsafe { GdiFlush() }, 0);
        // SAFETY: this thread owns an exact-sized, live DIB; input is distinct
        // Rust heap storage and no GDI operation can retain this bitmap.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.pixels.as_ptr(), self.length) };
    }

    fn verify(&self, bytes: &[u8]) {
        assert_ne!(unsafe { GdiFlush() }, 0);
        // SAFETY: the uniquely owned, flushed DIB is alive and exactly bounded.
        assert_eq!(unsafe { std::slice::from_raw_parts(self.pixels.as_ptr(), self.length) }, bytes);
    }
}

impl Drop for Bitmap {
    fn drop(&mut self) {
        // SAFETY: this standalone workload never selects the bitmap into a DC.
        assert_ne!(unsafe { DeleteObject(self.handle.as_ptr()) }, 0);
    }
}

fn sample(reuse: bool) {
    const WIDTH: u32 = 1024;
    const HEIGHT: u32 = 1024;
    let mut bytes = vec![255; WIDTH as usize * HEIGHT as usize * 4];
    let mut bitmap = Some(Bitmap::new(WIDTH, HEIGHT, &bytes));
    let mut samples = Vec::new();
    for index in 0..25 {
        bytes[..4].copy_from_slice(&[index, 23, 77, 255]);
        let start = Instant::now();
        if reuse {
            bitmap.as_ref().unwrap().refresh(&bytes);
        } else {
            drop(bitmap.take());
            bitmap = Some(Bitmap::new(WIDTH, HEIGHT, &bytes));
        }
        let elapsed = start.elapsed().as_secs_f64() * 1_000_000.;
        // Full byte checking/allocation of the input are outside the clock.
        bitmap.as_ref().unwrap().verify(&bytes);
        if index >= 5 {
            samples.push(elapsed);
        }
    }
    println!("{} us={samples:?}", if reuse { "reuse" } else { "reallocate" });
}

fn main() {
    assert_eq!(std::mem::size_of::<Header>(), 40);
    assert_eq!(std::mem::size_of::<Info>(), 44);
    for order in [[false, true], [true, false], [false, true]] {
        for reuse in order {
            sample(reuse);
        }
    }
}
