//! Validated preservation policy for WebGL2 framebuffer invalidation hints.
use super::{ApiVersion, Command, Result, WebGl, gl};
use serde_json::Value;

impl WebGl {
    pub(super) fn invalidate_framebuffer(&self, c: &Command) -> Result<Value> {
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        let attachment_start = if c.op == "invalidateSubFramebuffer" {
            // A signed origin outside the image is legal. Avoid adding width to
            // x (or height to y): the hint need not touch or allocate any region.
            c.n(1)?;
            c.n(2)?;
            if c.n(3)? < 0 || c.n(4)? < 0 {
                return Err(gl::INVALID_VALUE);
            }
            5
        } else {
            1
        };
        let default = self.core_framebuffer_id(c.u(0)?)? == 0;
        for index in attachment_start..c.i.len() {
            let point = c.u(index)?;
            if default {
                if ![0x1800, 0x1801, 0x1802].contains(&point) {
                    return Err(gl::INVALID_ENUM);
                }
            } else if (gl::COLOR_ATTACHMENT0..=gl::COLOR_ATTACHMENT0 + 31).contains(&point) {
                if !self.color_attachment_allowed(point) {
                    return Err(gl::INVALID_OPERATION);
                }
            } else if ![gl::DEPTH_ATTACHMENT, gl::STENCIL_ATTACHMENT, 0x821a].contains(&point) {
                return Err(gl::INVALID_ENUM);
            }
        }
        // WebGL explicitly permits a no-op: contents must remain unchanged or
        // be cleared to defined defaults, never expose native undefined storage.
        // ANGLE's GLES discard is not forwarded. This preserves all attachments,
        // including integer/HDR images, depth/stencil, absent images, and MSAA.
        // WebGL2 specification, "Framebuffer contents after invalidation":
        // https://registry.khronos.org/webgl/specs/latest/2.0/
        Ok(Value::Null)
    }
}
