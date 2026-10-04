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
