//! Closed texture-transfer routing keeps the generic command facade small.
use super::{ApiVersion, Command, Result, WebGl, gl};
use serde_json::Value;

impl WebGl {
    pub(super) fn dispatch_texture_transfer(
        &mut self,
        c: &Command,
        bytes: Option<&[u8]>,
    ) -> Option<Result<Value>> {
        Some(match c.op.as_str() {
            "texImage2DFromImage"
            | "texSubImage2DFromImage"
            | "texImage3DFromImage"
            | "texSubImage3DFromImage" => self.image_upload(c, bytes),
            "compressedTexImage2D"
            | "compressedTexSubImage2D"
            | "compressedTexImage2DFromBuffer"
            | "compressedTexSubImage2DFromBuffer" => self.compressed_texture_command(c, bytes),
            "compressedTexImage3D"
            | "compressedTexSubImage3D"
            | "compressedTexImage3DFromBuffer"
            | "compressedTexSubImage3DFromBuffer" => self.compressed_volume_command(c, bytes),
            "texImage2DFromBuffer" | "texSubImage2DFromBuffer" => {
                if self.options.api != ApiVersion::Two {
                    Err(gl::INVALID_OPERATION)
                } else {
                    self.core_texture_upload(c, bytes)
                }
            }
            "texImage3DFromBuffer"
            | "texSubImage3DFromBuffer"
            | "texStorage3D"
            | "texImage3D"
            | "texSubImage3D" => self.volume_texture_command(c, bytes),
            "copyTexSubImage3D" => self.copy_volume_texture(c),
            "texStorage2D" => self.core_texture_storage(c),
            _ => return None,
        })
    }
}
