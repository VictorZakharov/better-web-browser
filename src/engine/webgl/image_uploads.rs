//! DOM source conversion produces a tight owned region, independent of author
//! row/alignment state. The original pixel-store state survives every exit.
use super::{ApiVersion, Command, Result, WebGl, gl};
use serde_json::Value;

impl WebGl {
    pub(super) fn image_upload(&mut self, c: &Command, bytes: Option<&[u8]>) -> Result<Value> {
        if self.options.api != ApiVersion::Two || bytes.is_none() {
            return Err(gl::INVALID_OPERATION);
        }
        let op = match c.op.as_str() {
            "texImage2DFromImage" => "texImage2D",
            "texSubImage2DFromImage" => "texSubImage2D",
            "texImage3DFromImage" => "texImage3D",
            "texSubImage3DFromImage" => "texSubImage3D",
            _ => return Err(gl::INVALID_OPERATION),
        };
        let volume = op.ends_with("3D");
        let sub = op.starts_with("texSub");
        let internal = if sub {
            let target = c.u(0)?;
            let slot = if !volume && (0x8515..=0x851a).contains(&target) {
                1
            } else {
                self.texture_slot(target)?
            };
            let id = self.textures[self.texture_unit][slot];
            self.objects
                .get(id, super::Kind::Texture)?
                .core_images
                .get(&(target, c.n(1)?))
                .ok_or(gl::INVALID_OPERATION)?
                .internal
        } else {
            c.u(2)?
        };
        let format_index = if volume { if sub { 8 } else { 7 } } else { 6 };
        dom_upload(internal, c.u(format_index)?, c.u(format_index + 1)?)?;
        let command = Command {
            op: op.into(),
            i: c.i.clone(),
            f: vec![],
            text: String::new(),
        };
        let _store = super::pixel_store_guard::PixelStoreGuard::tight(
            self.options.api,
            super::pixel_layout::Direction::Unpack,
        );
        // Both existing upload paths still reject a bound unpack buffer. Never
        // unbind it here: a CPU image upload must not silently change overloads.
        if op.ends_with("3D") {
            self.volume_texture_command(&command, bytes)
        } else {
            self.core_texture_upload(&command, bytes)
        }
    }
}

fn dom_upload(internal: u32, format: u32, kind: u32) -> Result<()> {
    // WebGL2's TexImageSource table is deliberately narrower than GLES3's
    // client-memory upload table (in particular, no signed/integer expansion,
    // depth sources, RGB9_E5 packed words, or RGB5_A1's 2/10/10/10 route).
    // https://registry.khronos.org/webgl/specs/latest/2.0/#DOM-uploads
    super::core_texture_formats::upload(internal, format, kind)?;
    let valid = match internal {
        gl::ALPHA
        | gl::LUMINANCE
        | gl::LUMINANCE_ALPHA
        | 0x8229
        | 0x822b
        | 0x8051
        | 0x8c41
        | 0x8058
        | 0x8c43
        | 0x8232
        | 0x8238
        | 0x8d7d
        | 0x8d7c => kind == gl::UNSIGNED_BYTE,
        gl::RGB | 0x8d62 => [gl::UNSIGNED_BYTE, gl::UNSIGNED_SHORT_5_6_5].contains(&kind),
        gl::RGBA => [
            gl::UNSIGNED_BYTE,
            gl::UNSIGNED_SHORT_4_4_4_4,
            gl::UNSIGNED_SHORT_5_5_5_1,
        ]
        .contains(&kind),
        0x8056 => [gl::UNSIGNED_BYTE, gl::UNSIGNED_SHORT_4_4_4_4].contains(&kind),
        0x8057 => [gl::UNSIGNED_BYTE, gl::UNSIGNED_SHORT_5_5_5_1].contains(&kind),
        0x8059 => kind == 0x8368,
        0x8c3a => [0x8c3b, 0x140b, gl::FLOAT].contains(&kind),
        0x822d | 0x822f | 0x881b | 0x881a | 0x8c3d => [0x140b, gl::FLOAT].contains(&kind),
        0x822e | 0x8230 | 0x8815 | 0x8814 => kind == gl::FLOAT,
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(gl::INVALID_OPERATION)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dom_formats_are_not_the_entire_client_memory_table() {
        for (internal, format, kind) in [
            (0x8057, gl::RGBA, 0x8368),
            (0x8c3d, gl::RGB, 0x8c3e),
            (0x8f97, gl::RGBA, gl::BYTE),
            (0x8234, 0x8d94, gl::UNSIGNED_SHORT),
            (0x81a5, 0x1902, gl::UNSIGNED_SHORT),
        ] {
            assert!(super::super::core_texture_formats::upload(internal, format, kind).is_ok());
            assert_eq!(
                dom_upload(internal, format, kind),
                Err(gl::INVALID_OPERATION)
            );
        }
        for (internal, format, kind) in [
            (0x8057, gl::RGBA, gl::UNSIGNED_SHORT_5_5_5_1),
            (0x8059, gl::RGBA, 0x8368),
            (0x8c3a, gl::RGB, 0x8c3b),
            (0x8c3d, gl::RGB, gl::FLOAT),
            (0x8d7c, 0x8d99, gl::UNSIGNED_BYTE),
        ] {
            assert_eq!(dom_upload(internal, format, kind), Ok(()));
        }
    }
}
