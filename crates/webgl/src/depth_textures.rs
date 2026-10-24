//! WebGL1 depth textures are render-only, level-zero, two-dimensional images.
//! The private GLES3 provider permits uploads/copies that WEBGL_depth_texture
//! explicitly forbids; validate the portable public contract first.
//! https://registry.khronos.org/webgl/extensions/WEBGL_depth_texture/
use super::{Command, Result, gl};

pub(super) const DEPTH_COMPONENT: u32 = 0x1902;
pub(super) const DEPTH_STENCIL: u32 = 0x84f9;
pub(super) const UNSIGNED_INT_24_8: u32 = 0x84fa;

pub(super) fn is_depth(format: u32) -> bool {
    [DEPTH_COMPONENT, DEPTH_STENCIL].contains(&format)
}

pub(super) fn validate_upload(c: &Command, bytes: Option<&[u8]>) -> Result<()> {
    if !is_depth(c.u(6)?) {
        return Ok(());
    }
    if c.op != "texImage2D" || c.u(0)? != gl::TEXTURE_2D || c.n(1)? != 0 || bytes.is_some() {
        return Err(gl::INVALID_OPERATION);
    }
    Ok(())
}

pub(super) fn pixel_bytes(format: u32, kind: u32) -> Result<usize> {
    match (format, kind) {
        (DEPTH_COMPONENT, gl::UNSIGNED_SHORT) => Ok(2),
        (DEPTH_COMPONENT, gl::UNSIGNED_INT) | (DEPTH_STENCIL, UNSIGNED_INT_24_8) => Ok(4),
        _ => Err(gl::INVALID_OPERATION),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_format_type_table_is_closed() {
        for (format, kind, bytes) in [
            (DEPTH_COMPONENT, gl::UNSIGNED_SHORT, 2),
            (DEPTH_COMPONENT, gl::UNSIGNED_INT, 4),
            (DEPTH_STENCIL, UNSIGNED_INT_24_8, 4),
        ] {
            assert_eq!(pixel_bytes(format, kind), Ok(bytes));
        }
        for (format, kind) in [
            (DEPTH_COMPONENT, gl::FLOAT),
            (DEPTH_COMPONENT, UNSIGNED_INT_24_8),
            (DEPTH_STENCIL, gl::UNSIGNED_SHORT),
            (DEPTH_STENCIL, gl::UNSIGNED_INT),
            (gl::RGBA, UNSIGNED_INT_24_8),
        ] {
            assert_eq!(pixel_bytes(format, kind), Err(gl::INVALID_OPERATION));
        }
    }
}
