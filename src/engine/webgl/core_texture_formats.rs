//! Closed GLES3 texture combinations, independently checked before native reads.
//! Khronos GLES3 glTexImage2D tables 1/2 and WebGL2 §3.7.6.
use super::{Result, gl};

pub(super) const RED: u32 = 0x1903;
pub(super) const RG: u32 = 0x8227;
pub(super) const RED_INTEGER: u32 = 0x8d94;
pub(super) const RG_INTEGER: u32 = 0x8228;
pub(super) const RGB_INTEGER: u32 = 0x8d98;
pub(super) const RGBA_INTEGER: u32 = 0x8d99;
pub(super) const HALF: u32 = 0x140b;
const UINT_2101010: u32 = 0x8368;
const UINT_101111: u32 = 0x8c3b;
const UINT_5999: u32 = 0x8c3e;
const UINT_248: u32 = 0x84fa;
const FLOAT_UINT_248: u32 = 0x8dad;

#[derive(Clone, Copy, Debug)]
pub(super) struct StorageFormat {
    pub base: u32,
    pub types: &'static [u32],
    /// Charge expanded ANGLE RGBA storage even for one/three-channel formats.
    pub bytes: usize,
}

pub(super) fn storage(internal: u32) -> Result<StorageFormat> {
    let (base, types, bytes): (u32, &'static [u32], usize) = match internal {
        0x8229 => (RED, &[gl::UNSIGNED_BYTE], 4),              // R8
        0x822b => (RG, &[gl::UNSIGNED_BYTE], 4),               // RG8
        0x8051 | 0x8c41 => (gl::RGB, &[gl::UNSIGNED_BYTE], 4), // RGB8/sRGB8
        0x8058 | 0x8c43 => (gl::RGBA, &[gl::UNSIGNED_BYTE], 4),
        0x8f94 => (RED, &[gl::BYTE], 4),
        0x8f95 => (RG, &[gl::BYTE], 4),
        0x8f96 => (gl::RGB, &[gl::BYTE], 4),
        0x8f97 => (gl::RGBA, &[gl::BYTE], 4),
        0x822a => (RED, &[gl::UNSIGNED_SHORT], 8),
        0x822c => (RG, &[gl::UNSIGNED_SHORT], 8),
        0x8054 => (gl::RGB, &[gl::UNSIGNED_SHORT], 8),
        0x805b => (gl::RGBA, &[gl::UNSIGNED_SHORT], 8),
        0x8f98 => (RED, &[gl::SHORT], 8),
        0x8f99 => (RG, &[gl::SHORT], 8),
        0x8f9a => (gl::RGB, &[gl::SHORT], 8),
        0x8f9b => (gl::RGBA, &[gl::SHORT], 8),
        0x822d => (RED, &[HALF, gl::FLOAT], 8),
        0x822f => (RG, &[HALF, gl::FLOAT], 8),
        0x881b => (gl::RGB, &[HALF, gl::FLOAT], 8),
        0x881a => (gl::RGBA, &[HALF, gl::FLOAT], 8),
        0x822e => (RED, &[gl::FLOAT], 16),
        0x8230 => (RG, &[gl::FLOAT], 16),
        0x8815 => (gl::RGB, &[gl::FLOAT], 16),
        0x8814 => (gl::RGBA, &[gl::FLOAT], 16),
        0x8d62 => (gl::RGB, &[gl::UNSIGNED_BYTE, gl::UNSIGNED_SHORT_5_6_5], 4),
        0x8056 => (
            gl::RGBA,
            &[gl::UNSIGNED_BYTE, gl::UNSIGNED_SHORT_4_4_4_4],
            4,
        ),
        0x8057 => (
            gl::RGBA,
            &[gl::UNSIGNED_BYTE, gl::UNSIGNED_SHORT_5_5_5_1, UINT_2101010],
            4,
        ),
        0x8059 => (gl::RGBA, &[UINT_2101010], 4),
        0x8c3a => (gl::RGB, &[UINT_101111, HALF, gl::FLOAT], 4),
        0x8c3d => (gl::RGB, &[UINT_5999, HALF, gl::FLOAT], 4),
        0x8231 => (RED_INTEGER, &[gl::BYTE], 4),
        0x8232 => (RED_INTEGER, &[gl::UNSIGNED_BYTE], 4),
        0x8233 => (RED_INTEGER, &[gl::SHORT], 8),
        0x8234 => (RED_INTEGER, &[gl::UNSIGNED_SHORT], 8),
        0x8235 => (RED_INTEGER, &[gl::INT], 16),
        0x8236 => (RED_INTEGER, &[gl::UNSIGNED_INT], 16),
        0x8237 => (RG_INTEGER, &[gl::BYTE], 4),
        0x8238 => (RG_INTEGER, &[gl::UNSIGNED_BYTE], 4),
        0x8239 => (RG_INTEGER, &[gl::SHORT], 8),
        0x823a => (RG_INTEGER, &[gl::UNSIGNED_SHORT], 8),
        0x823b => (RG_INTEGER, &[gl::INT], 16),
        0x823c => (RG_INTEGER, &[gl::UNSIGNED_INT], 16),
        0x8d8f => (RGB_INTEGER, &[gl::BYTE], 4),
        0x8d7d => (RGB_INTEGER, &[gl::UNSIGNED_BYTE], 4),
        0x8d89 => (RGB_INTEGER, &[gl::SHORT], 8),
        0x8d77 => (RGB_INTEGER, &[gl::UNSIGNED_SHORT], 8),
        0x8d83 => (RGB_INTEGER, &[gl::INT], 16),
        0x8d71 => (RGB_INTEGER, &[gl::UNSIGNED_INT], 16),
        0x8d8e => (RGBA_INTEGER, &[gl::BYTE], 4),
        0x8d7c => (RGBA_INTEGER, &[gl::UNSIGNED_BYTE], 4),
        0x8d88 => (RGBA_INTEGER, &[gl::SHORT], 8),
        0x8d76 => (RGBA_INTEGER, &[gl::UNSIGNED_SHORT], 8),
        0x8d82 => (RGBA_INTEGER, &[gl::INT], 16),
        0x8d70 => (RGBA_INTEGER, &[gl::UNSIGNED_INT], 16),
        0x906f => (RGBA_INTEGER, &[UINT_2101010], 4),
        0x81a5 => (0x1902, &[gl::UNSIGNED_SHORT, gl::UNSIGNED_INT], 4),
        0x81a6 => (0x1902, &[gl::UNSIGNED_INT], 4),
        0x8cac => (0x1902, &[gl::FLOAT], 4),
        0x88f0 => (0x84f9, &[UINT_248], 4),
        0x8cad => (0x84f9, &[FLOAT_UINT_248], 8),
        _ => return Err(gl::INVALID_ENUM),
    };
    Ok(StorageFormat { base, types, bytes })
}

pub(super) fn upload(internal: u32, format: u32, kind: u32) -> Result<(usize, usize)> {
    let bytes = upload_bytes(format, kind)?;
    if [
        gl::ALPHA,
        gl::LUMINANCE,
        gl::LUMINANCE_ALPHA,
        gl::RGB,
        gl::RGBA,
    ]
    .contains(&internal)
    {
        if internal != format {
            return Err(gl::INVALID_OPERATION);
        }
        let valid = kind == gl::UNSIGNED_BYTE
            || format == gl::RGB && kind == gl::UNSIGNED_SHORT_5_6_5
            || format == gl::RGBA
                && [gl::UNSIGNED_SHORT_4_4_4_4, gl::UNSIGNED_SHORT_5_5_5_1].contains(&kind);
        return if valid {
            Ok((bytes, 4))
        } else {
            Err(gl::INVALID_OPERATION)
        };
    }
    // TexImage's internalformat is GLint, unlike TexStorage's GLenum. Preserve
    // its INVALID_VALUE contract for unsupported values (GLES2 §3.7.1 and
    // Chromium ValidateTexFuncFormatAndType); storage queries retain ENUM.
    let storage = storage(internal).map_err(|_| gl::INVALID_VALUE)?;
    if storage.base != format || !storage.types.contains(&kind) {
        return Err(gl::INVALID_OPERATION);
    }
    Ok((bytes, storage.bytes))
}

fn upload_bytes(format: u32, kind: u32) -> Result<usize> {
    let components = match format {
        RED | RED_INTEGER | gl::ALPHA | gl::LUMINANCE | 0x1902 => 1,
        RG | RG_INTEGER | gl::LUMINANCE_ALPHA => 2,
        gl::RGB | RGB_INTEGER => 3,
        gl::RGBA | RGBA_INTEGER => 4,
        0x84f9 => 1,
        _ => return Err(gl::INVALID_ENUM),
    };
    Ok(match kind {
        gl::BYTE | gl::UNSIGNED_BYTE => components,
        gl::SHORT | gl::UNSIGNED_SHORT | HALF => components * 2,
        gl::INT | gl::UNSIGNED_INT | gl::FLOAT => components * 4,
        gl::UNSIGNED_SHORT_5_6_5 | gl::UNSIGNED_SHORT_4_4_4_4 | gl::UNSIGNED_SHORT_5_5_5_1 => 2,
        UINT_2101010 | UINT_101111 | UINT_5999 | UINT_248 => 4,
        FLOAT_UINT_248 => 8,
        _ => return Err(gl::INVALID_ENUM),
    })
}

pub(super) fn read_bytes(format: u32, kind: u32) -> Result<usize> {
    upload_bytes(format, kind)
}

pub(super) fn validate_read_enums(format: u32, kind: u32) -> Result<()> {
    if ![
        RED,
        RG,
        gl::RGB,
        gl::RGBA,
        gl::ALPHA,
        RED_INTEGER,
        RG_INTEGER,
        RGB_INTEGER,
        RGBA_INTEGER,
    ]
    .contains(&format)
        || ![
            gl::BYTE,
            gl::UNSIGNED_BYTE,
            gl::SHORT,
            gl::UNSIGNED_SHORT,
            gl::INT,
            gl::UNSIGNED_INT,
            HALF,
            gl::FLOAT,
            gl::UNSIGNED_SHORT_5_6_5,
            gl::UNSIGNED_SHORT_4_4_4_4,
            gl::UNSIGNED_SHORT_5_5_5_1,
            UINT_2101010,
            UINT_101111,
            UINT_5999,
        ]
        .contains(&kind)
    {
        return Err(gl::INVALID_ENUM);
    }
    Ok(())
}
