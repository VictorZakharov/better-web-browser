//! Native compositor devices and bounded, validated shared Canvas imports.
//! Existing windows 0.62 bindings are MIT/Apache-2.0; no additional dependency.
mod device;
mod fence;
mod frames;
pub use fence::Fence;
mod target;
pub use device::Device;
pub use frames::Frames;
pub use target::Target;

use windows::Win32::Graphics::{
    Direct2D::Common::*, Direct2D::*, Direct3D11::*, Dxgi::Common::*, Dxgi::*,
};
use windows::core::Interface;

const MAX_BYTES: usize = 64 * 1024 * 1024;

fn bitmap_properties(options: D2D1_BITMAP_OPTIONS) -> D2D1_BITMAP_PROPERTIES1 {
    D2D1_BITMAP_PROPERTIES1 {
        pixelFormat: D2D1_PIXEL_FORMAT {
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
        },
        dpiX: 96.0,
        dpiY: 96.0,
        bitmapOptions: options,
        ..Default::default()
    }
}
