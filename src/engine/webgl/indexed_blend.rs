//! Per-output blend state stays owned by ANGLE, including global-state broadcasts.
//! WebGL extension revision 6: https://registry.khronos.org/webgl/extensions/OES_draw_buffers_indexed/
use super::{ApiVersion, Command, Result, WebGl, gl};
use serde_json::{Value, json};

#[cfg(test)]
mod tests;

pub(super) fn query_allowed(target: u32) -> bool {
    matches!(
        target,
        0x8009 | 0x883d | 0x80c8 | 0x80c9 | 0x80ca | 0x80cb | gl::COLOR_WRITEMASK
    )
}

impl WebGl {
    pub(super) fn indexed_blend_command(&mut self, c: &Command) -> Result<Value> {
        if self.options.api != ApiVersion::Two || !self.extensions.indexed_blend {
            return Err(gl::INVALID_OPERATION);
        }
        let entries = self
            .extensions
            .indexed_blend_entries
            .ok_or(gl::INVALID_OPERATION)?;
        let query = c.op == "getIndexedParameter";
        let toggle = matches!(c.op.as_str(), "enableiOES" | "disableiOES");
        let index = c.u(usize::from(query || toggle))?;
        if query && !query_allowed(c.u(0)?) || toggle && c.u(0)? != gl::BLEND {
            return Err(gl::INVALID_ENUM);
        }
        if index >= self.extensions.max_draw_buffers {
            return Err(gl::INVALID_VALUE);
        }
        // SAFETY: closed signatures and index bound, owned correctly sized
        // output arrays. Native WebGL compatibility validates factors, equations
        // and simultaneous constant-color/alpha use across active draw buffers.
        unsafe {
            match c.op.as_str() {
                "getIndexedParameter" => {
                    let target = c.u(0)?;
                    let result = if target == gl::COLOR_WRITEMASK {
                        let mut mask = [0; 4];
                        (entries.boolean)(target, index, mask.as_mut_ptr());
                        json!(mask.map(|channel| channel != 0))
                    } else {
                        let mut value = 0;
                        (entries.integer)(target, index, &mut value);
                        json!(value)
                    };
                    self.driver_result()?;
                    return Ok(result);
                }
                "enableiOES" => (entries.enable)(gl::BLEND, index),
                "disableiOES" => (entries.disable)(gl::BLEND, index),
                "blendEquationiOES" => (entries.equation)(index, c.u(1)?),
                "blendEquationSeparateiOES" => (entries.equation_separate)(index, c.u(1)?, c.u(2)?),
                "blendFunciOES" => (entries.function)(index, c.u(1)?, c.u(2)?),
                "blendFuncSeparateiOES" => {
                    (entries.function_separate)(index, c.u(1)?, c.u(2)?, c.u(3)?, c.u(4)?)
                }
                "colorMaskiOES" => (entries.mask)(
                    index,
                    u8::from(c.u(1)? != 0),
                    u8::from(c.u(2)? != 0),
                    u8::from(c.u(3)? != 0),
                    u8::from(c.u(4)? != 0),
                ),
                _ => return Err(gl::INVALID_OPERATION),
            }
        }
        self.driver_result()?;
        Ok(Value::Null)
    }
}
