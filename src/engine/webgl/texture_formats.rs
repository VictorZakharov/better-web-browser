//! Closed WebGL 1 format/type table and conservative native storage accounting.
//! https://registry.khronos.org/webgl/extensions/OES_texture_float/
//! https://registry.khronos.org/webgl/extensions/OES_texture_half_float/
use super::texture_capabilities::{TextureCapabilities, TextureCapability as Capability};
use super::{Result, gl};

pub(super) const HALF_FLOAT: u32 = 0x8d61;
pub(super) const RGBA32F: u32 = 0x8814;
pub(super) const RGBA16F: u32 = 0x881a;
pub(super) const RGB16F: u32 = 0x881b;
pub(super) const COMPONENT_TYPE: u32 = 0x8211;
const RED: u32 = 0x1903;
const GREEN: u32 = 0x1904;
const BLUE: u32 = 0x1905;

pub(super) fn native_format(format: u32, kind: u32) -> (u32, u32, u32) {
    if super::depth_textures::is_depth(format) {
        // Keep WebGL1's unsized depth definition. ANGLE deliberately permits
        // legacy non-comparison LINEAR sampling for these formats; sized GLES3
        // depth definitions instead require NEAREST (crbug.com/649200).
        return (format, format, kind);
    }
    let native_kind = if kind == HALF_FLOAT { 0x140b } else { kind };
    if ![gl::FLOAT, HALF_FLOAT].contains(&kind) {
        return (format, format, native_kind);
    }
    let half = kind == HALF_FLOAT;
    let (internal, base) = match format {
        gl::RGBA => (if half { RGBA16F } else { RGBA32F }, gl::RGBA),
        gl::RGB => (if half { RGB16F } else { 0x8815 }, gl::RGB),
        gl::ALPHA | gl::LUMINANCE => (if half { 0x822d } else { 0x822e }, RED),
        gl::LUMINANCE_ALPHA => (if half { 0x822f } else { 0x8230 }, 0x8227),
        _ => (format, format),
    };
    (internal, base, native_kind)
}

pub(super) fn native_swizzle(format: u32, kind: u32) -> [u32; 4] {
    if [gl::FLOAT, HALF_FLOAT].contains(&kind) {
        match format {
            gl::ALPHA => return [gl::ZERO, gl::ZERO, gl::ZERO, RED],
            gl::LUMINANCE => return [RED, RED, RED, gl::ONE],
            gl::LUMINANCE_ALPHA => return [RED, RED, RED, GREEN],
            _ => {}
        }
    }
    [RED, GREEN, BLUE, gl::ALPHA]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PixelFormat {
    pub upload_bytes: usize,
    pub storage_bytes: usize,
}

pub(super) fn texture_format(
    format: u32,
    kind: u32,
    capabilities: &TextureCapabilities,
) -> Result<PixelFormat> {
    if super::depth_textures::is_depth(format) {
        if !capabilities.enabled(Capability::Depth) {
            return Err(gl::INVALID_ENUM);
        }
        return Ok(PixelFormat {
            upload_bytes: super::depth_textures::pixel_bytes(format, kind)?,
            storage_bytes: 4,
        });
    }
    if capabilities.enabled(Capability::Depth)
        && [
            gl::UNSIGNED_SHORT,
            gl::UNSIGNED_INT,
            super::depth_textures::UNSIGNED_INT_24_8,
        ]
        .contains(&kind)
    {
        return Err(gl::INVALID_OPERATION);
    }
    let components = match format {
        gl::RGBA => 4,
        gl::RGB => 3,
        gl::LUMINANCE_ALPHA => 2,
        gl::ALPHA | gl::LUMINANCE => 1,
        _ => return Err(gl::INVALID_ENUM),
    };
    let (upload_bytes, storage_bytes) = match kind {
        gl::UNSIGNED_BYTE => (components, 4),
        gl::UNSIGNED_SHORT_5_6_5 if format == gl::RGB => (2, 4),
        gl::UNSIGNED_SHORT_4_4_4_4 | gl::UNSIGNED_SHORT_5_5_5_1 if format == gl::RGBA => (2, 4),
        gl::FLOAT => {
            if !capabilities.enabled(Capability::Float) {
                return Err(gl::INVALID_ENUM);
            }
            (components * 4, 16)
        }
        HALF_FLOAT => {
            if !capabilities.enabled(Capability::HalfFloat) {
                return Err(gl::INVALID_ENUM);
            }
            (components * 2, 8)
        }
        gl::UNSIGNED_SHORT_5_6_5 | gl::UNSIGNED_SHORT_4_4_4_4 | gl::UNSIGNED_SHORT_5_5_5_1 => {
            return Err(gl::INVALID_OPERATION);
        }
        _ => return Err(gl::INVALID_ENUM),
    };
    // Legacy luminance/alpha and RGB formats can require RGBA storage in ANGLE.
    // Charge for the expanded native format, not only the author upload bytes.
    Ok(PixelFormat {
        upload_bytes,
        storage_bytes,
    })
}

pub(super) fn renderbuffer_bytes(format: u32, capabilities: &TextureCapabilities) -> Result<usize> {
    match format {
        gl::RGBA4
        | gl::RGB565
        | gl::RGB5_A1
        | gl::DEPTH_COMPONENT16
        | gl::STENCIL_INDEX8
        | 0x84f9 => Ok(4),
        RGBA32F if capabilities.enabled(Capability::ColorFloat) => Ok(16),
        RGBA16F | RGB16F if capabilities.enabled(Capability::ColorHalfFloat) => Ok(8),
        _ => Err(gl::INVALID_ENUM),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled() -> TextureCapabilities {
        let mut result = TextureCapabilities::discover(
            Capability::ALL
                .iter()
                .map(|value| value.native_name().to_str().unwrap()),
        );
        for capability in Capability::ALL {
            result.confirm(capability, true);
        }
        result
    }

    #[test]
    fn float_formats_are_not_admitted_from_native_availability_alone() {
        let available = TextureCapabilities::discover(
            ["GL_OES_texture_float", "GL_OES_texture_half_float"].into_iter(),
        );
        assert_eq!(
            texture_format(gl::RGBA, gl::FLOAT, &available),
            Err(gl::INVALID_ENUM)
        );
        assert_eq!(
            texture_format(gl::RGBA, HALF_FLOAT, &available),
            Err(gl::INVALID_ENUM)
        );
    }

    #[test]
    fn expanded_storage_cannot_be_undercharged_by_legacy_formats() {
        let capabilities = enabled();
        for (format, components) in [
            (gl::RGBA, 4),
            (gl::RGB, 3),
            (gl::LUMINANCE_ALPHA, 2),
            (gl::ALPHA, 1),
            (gl::LUMINANCE, 1),
        ] {
            for (kind, component_bytes, storage) in [
                (gl::UNSIGNED_BYTE, 1, 4),
                (HALF_FLOAT, 2, 8),
                (gl::FLOAT, 4, 16),
            ] {
                let pixel = texture_format(format, kind, &capabilities).unwrap();
                assert_eq!(pixel.upload_bytes, components * component_bytes);
                assert_eq!(pixel.storage_bytes, storage);
            }
        }
    }

    #[test]
    fn packed_types_have_only_their_specified_base_format() {
        let capabilities = enabled();
        assert_eq!(
            texture_format(gl::RGB, gl::UNSIGNED_SHORT_5_6_5, &capabilities)
                .unwrap()
                .upload_bytes,
            2
        );
        assert_eq!(
            texture_format(gl::RGBA, gl::UNSIGNED_SHORT_5_6_5, &capabilities),
            Err(gl::INVALID_OPERATION)
        );
        for kind in [gl::UNSIGNED_SHORT_4_4_4_4, gl::UNSIGNED_SHORT_5_5_5_1] {
            assert_eq!(
                texture_format(gl::RGBA, kind, &capabilities)
                    .unwrap()
                    .upload_bytes,
                2
            );
            assert_eq!(
                texture_format(gl::RGB, kind, &capabilities),
                Err(gl::INVALID_OPERATION)
            );
        }
        assert_eq!(
            texture_format(0xdead, gl::UNSIGNED_BYTE, &capabilities),
            Err(gl::INVALID_ENUM)
        );
        assert_eq!(
            texture_format(gl::RGBA, 0xdead, &capabilities),
            Err(gl::INVALID_ENUM)
        );
    }

    #[test]
    fn float_renderbuffer_allocation_is_extension_gated_and_full_width() {
        let off = TextureCapabilities::default();
        let on = enabled();
        for format in [RGBA16F, RGB16F, RGBA32F] {
            assert_eq!(renderbuffer_bytes(format, &off), Err(gl::INVALID_ENUM));
        }
        assert_eq!(renderbuffer_bytes(RGBA16F, &on), Ok(8));
        assert_eq!(renderbuffer_bytes(RGB16F, &on), Ok(8));
        assert_eq!(renderbuffer_bytes(RGBA32F, &on), Ok(16));
        // WebGL_color_buffer_float does not expose RGB32F renderbuffer storage.
        assert_eq!(renderbuffer_bytes(0x8815, &on), Err(gl::INVALID_ENUM));
    }
}
