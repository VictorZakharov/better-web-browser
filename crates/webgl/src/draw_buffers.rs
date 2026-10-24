//! WEBGL_draw_buffers owns framebuffer-local routing, not global author state.
//! The private default surface maps BACK to its hidden COLOR_ATTACHMENT0.
//! https://registry.khronos.org/webgl/extensions/WEBGL_draw_buffers/
use super::{Command, Kind, Result, WebGl, gl, json};
use serde_json::Value;

pub(super) const MAX_DRAW_BUFFERS: u32 = 0x8824;
pub(super) const MAX_COLOR_ATTACHMENTS: u32 = 0x8cdf;
pub(super) const DRAW_BUFFER0: u32 = 0x8825;

impl WebGl {
    pub(super) fn color_attachment_allowed(&self, point: u32) -> bool {
        point == gl::COLOR_ATTACHMENT0
            || self.extensions.draw_buffers
                && point
                    .checked_sub(gl::COLOR_ATTACHMENT0)
                    .is_some_and(|index| index < self.extensions.max_color_attachments)
    }

    pub(super) fn draw_buffers_command(&mut self, c: &Command) -> Result<Value> {
        if !self.extensions.draw_buffers {
            return Err(gl::INVALID_OPERATION);
        }
        let buffers: Vec<_> = (0..c.i.len())
            .map(|index| c.u(index))
            .collect::<Result<_>>()?;
        if buffers.len() > self.extensions.max_draw_buffers as usize {
            return Err(gl::INVALID_VALUE);
        }
        let mut native = buffers.clone();
        if self.framebuffer == 0 {
            if buffers.len() != 1 {
                return Err(gl::INVALID_OPERATION);
            }
            if ![gl::BACK, gl::NONE].contains(&buffers[0]) {
                return Err(gl::INVALID_OPERATION);
            }
            if buffers[0] == gl::BACK {
                native[0] = gl::COLOR_ATTACHMENT0;
            }
        } else if buffers.iter().enumerate().any(|(index, &value)| {
            value != gl::NONE && value != gl::COLOR_ATTACHMENT0 + index as u32
        }) {
            return Err(gl::INVALID_OPERATION);
        }
        let entry = self
            .extensions
            .draw_buffers_entry
            .ok_or(gl::INVALID_OPERATION)?;
        // SAFETY: at most 16 closed enums; the slice owns the full native input.
        unsafe {
            entry(native.len() as i32, native.as_ptr());
        }
        self.driver_result()?;
        if self.framebuffer == 0 {
            self.default_draw_buffer = buffers[0];
        } else {
            self.objects
                .get_mut(self.framebuffer, Kind::Framebuffer)?
                .draw_buffers = buffers;
        }
        Ok(Value::Null)
    }

    pub(super) fn draw_buffer_parameter(&self, pname: u32) -> Result<Option<Value>> {
        if ![MAX_DRAW_BUFFERS, MAX_COLOR_ATTACHMENTS].contains(&pname)
            && !(DRAW_BUFFER0..DRAW_BUFFER0 + 16).contains(&pname)
        {
            return Ok(None);
        }
        if !self.extensions.draw_buffers {
            return Err(gl::INVALID_ENUM);
        }
        let value = match pname {
            MAX_DRAW_BUFFERS => self.extensions.max_draw_buffers,
            MAX_COLOR_ATTACHMENTS => self.extensions.max_color_attachments,
            _ => {
                let index = pname - DRAW_BUFFER0;
                if index >= self.extensions.max_draw_buffers {
                    return Err(gl::INVALID_ENUM);
                }
                if self.framebuffer == 0 {
                    if index == 0 {
                        self.default_draw_buffer
                    } else {
                        gl::NONE
                    }
                } else {
                    self.objects
                        .get(self.framebuffer, Kind::Framebuffer)?
                        .draw_buffers
                        .get(index as usize)
                        .copied()
                        .unwrap_or(gl::NONE)
                }
            }
        };
        Ok(Some(json!(value)))
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{command, context};
    use super::*;

    #[test]
    fn native_mrt_disabled_default_route_cannot_prevent_compositor_retirement_clear() {
        let (mut contexts, id) = context();
        assert_eq!(
            command(
                &mut contexts,
                id,
                "enableExtension",
                &[],
                &[],
                "WEBGL_draw_buffers",
                None
            ),
            json!(true)
        );
        command(
            &mut contexts,
            id,
            "clearColor",
            &[],
            &[1.0, 0.0, 0.0, 1.0],
            "",
            None,
        );
        command(
            &mut contexts,
            id,
            "clear",
            &[gl::COLOR_BUFFER_BIT],
            &[],
            "",
            None,
        );
        assert_eq!(&contexts.snapshot(id).unwrap().2[..4], &[255, 0, 0, 255]);
        command(
            &mut contexts,
            id,
            "drawBuffersWEBGL",
            &[gl::NONE],
            &[],
            "",
            None,
        );
        command(&mut contexts, id, "presented", &[], &[], "", None);
        assert!(
            contexts
                .snapshot(id)
                .unwrap()
                .2
                .iter()
                .all(|value| *value == 0)
        );
        assert_eq!(
            command(
                &mut contexts,
                id,
                "getParameter",
                &[DRAW_BUFFER0],
                &[],
                "",
                None
            ),
            json!(gl::NONE)
        );
        command(&mut contexts, id, "resize", &[8, 8], &[], "", None);
        assert_eq!(
            command(
                &mut contexts,
                id,
                "getParameter",
                &[DRAW_BUFFER0],
                &[],
                "",
                None
            ),
            json!(gl::NONE)
        );
        command(
            &mut contexts,
            id,
            "clear",
            &[gl::COLOR_BUFFER_BIT],
            &[],
            "",
            None,
        );
        assert!(
            contexts
                .snapshot(id)
                .unwrap()
                .2
                .iter()
                .all(|value| *value == 0)
        );
        assert_eq!(
            command(&mut contexts, id, "getError", &[], &[], "", None),
            json!(gl::NO_ERROR)
        );
    }

    #[test]
    fn native_mrt_hostile_enums_and_oversized_routes_cannot_reach_unbounded_driver_calls() {
        let (mut contexts, id) = context();
        command(
            &mut contexts,
            id,
            "drawBuffersWEBGL",
            &[gl::BACK],
            &[],
            "",
            None,
        );
        assert_eq!(
            command(&mut contexts, id, "getError", &[], &[], "", None),
            json!(gl::INVALID_OPERATION)
        );
        command(
            &mut contexts,
            id,
            "enableExtension",
            &[],
            &[],
            "WEBGL_draw_buffers",
            None,
        );
        let buffers = [gl::NONE; 17];
        command(
            &mut contexts,
            id,
            "drawBuffersWEBGL",
            &buffers,
            &[],
            "",
            None,
        );
        assert_eq!(
            command(&mut contexts, id, "getError", &[], &[], "", None),
            json!(gl::INVALID_VALUE)
        );
        command(
            &mut contexts,
            id,
            "drawBuffersWEBGL",
            &[u32::MAX],
            &[],
            "",
            None,
        );
        assert_eq!(
            command(&mut contexts, id, "getError", &[], &[], "", None),
            json!(gl::INVALID_OPERATION)
        );
        assert_eq!(
            command(
                &mut contexts,
                id,
                "getParameter",
                &[DRAW_BUFFER0],
                &[],
                "",
                None
            ),
            json!(gl::BACK)
        );
        assert!(
            contexts
                .snapshot(id)
                .unwrap()
                .2
                .iter()
                .all(|value| *value == 0)
        );
    }
}
