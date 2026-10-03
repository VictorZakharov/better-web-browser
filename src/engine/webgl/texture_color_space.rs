//! EXT_sRGB color storage delegates transfer functions and linear-space blending
//! to ANGLE. Public WebGL1 formats remain narrower than the GLES3 storage table.
//! https://registry.khronos.org/webgl/extensions/EXT_sRGB/
use super::texture_capabilities::{TextureCapabilities, TextureCapability};
use super::texture_formats::PixelFormat;
use super::{Result, gl};

pub(super) const SRGB: u32 = 0x8c40;
pub(super) const SRGB_ALPHA: u32 = 0x8c42;
pub(super) const SRGB8_ALPHA8: u32 = 0x8c43;
pub(super) const COLOR_ENCODING: u32 = 0x8210;

pub(super) fn is_srgb(format: u32) -> bool {
    [SRGB, SRGB_ALPHA, SRGB8_ALPHA8].contains(&format)
}

pub(super) fn texture_format(
    format: u32,
    kind: u32,
    capabilities: &TextureCapabilities,
) -> Option<Result<PixelFormat>> {
    if ![SRGB, SRGB_ALPHA].contains(&format) {
        return None;
    }
    Some(if !capabilities.enabled(TextureCapability::Srgb) {
        Err(gl::INVALID_ENUM)
    } else if kind != gl::UNSIGNED_BYTE {
        Err(gl::INVALID_OPERATION)
    } else {
        Ok(PixelFormat {
            upload_bytes: if format == SRGB { 3 } else { 4 },
            storage_bytes: 4,
        })
    })
}

pub(super) fn native_format(format: u32) -> Option<(u32, u32, u32)> {
    match format {
        SRGB => Some((0x8c41 /* SRGB8 */, gl::RGB, gl::UNSIGNED_BYTE)),
        SRGB_ALPHA => Some((SRGB8_ALPHA8, gl::RGBA, gl::UNSIGNED_BYTE)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_is_an_extension_gated_byte_only_format_with_expanded_storage() {
        let mut capabilities = TextureCapabilities::discover(["GL_EXT_sRGB"].into_iter());
        for format in [SRGB, SRGB_ALPHA] {
            assert_eq!(
                texture_format(format, gl::UNSIGNED_BYTE, &capabilities),
                Some(Err(gl::INVALID_ENUM))
            );
        }
        capabilities.confirm(TextureCapability::Srgb, true);
        for (format, bytes) in [(SRGB, 3), (SRGB_ALPHA, 4)] {
            assert_eq!(
                texture_format(format, gl::UNSIGNED_BYTE, &capabilities),
                Some(Ok(PixelFormat {
                    upload_bytes: bytes,
                    storage_bytes: 4
                }))
            );
            assert_eq!(
                texture_format(format, gl::FLOAT, &capabilities),
                Some(Err(gl::INVALID_OPERATION))
            );
        }
        assert!(texture_format(SRGB8_ALPHA8, gl::UNSIGNED_BYTE, &capabilities).is_none());
        assert!(native_format(gl::RGBA).is_none());
    }
}
