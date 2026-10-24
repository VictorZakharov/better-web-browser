//! GLES2's signed GetIntegerv saturates unsigned masks above i32::MAX.
//! Keep the exact successful author values, including across private clears.
use super::{Command, Result, WebGl, gl};

#[derive(Clone, Copy)]
pub(super) struct StencilMasks {
    pub write: [u32; 2],
    value: [u32; 2],
}

impl Default for StencilMasks {
    fn default() -> Self {
        Self {
            write: [u32::MAX; 2],
            value: [u32::MAX; 2],
        }
    }
}

impl StencilMasks {
    pub(super) fn query(&self, pname: u32) -> Option<u32> {
        match pname {
            gl::STENCIL_WRITEMASK => Some(self.write[0]),
            gl::STENCIL_BACK_WRITEMASK => Some(self.write[1]),
            gl::STENCIL_VALUE_MASK => Some(self.value[0]),
            gl::STENCIL_BACK_VALUE_MASK => Some(self.value[1]),
            _ => None,
        }
    }
}

impl WebGl {
    pub(super) fn stencil_mask_command(&mut self, c: &Command) -> Result<()> {
        let (face, mask, write) = match c.op.as_str() {
            "stencilMask" => (gl::FRONT_AND_BACK, c.u(0)?, true),
            "stencilMaskSeparate" => (c.u(0)?, c.u(1)?, true),
            "stencilFunc" => (gl::FRONT_AND_BACK, c.u(2)?, false),
            _ => (c.u(0)?, c.u(3)?, false),
        };
        if ![gl::FRONT, gl::BACK, gl::FRONT_AND_BACK].contains(&face) {
            return Err(gl::INVALID_ENUM);
        }
        unsafe {
            if write {
                gl::StencilMaskSeparate(face, mask);
            } else {
                let (function, reference) = if c.op == "stencilFunc" {
                    (c.u(0)?, c.n(1)?)
                } else {
                    (c.u(1)?, c.n(2)?)
                };
                gl::StencilFuncSeparate(face, function, reference, mask);
            }
        }
        self.driver_result()?;
        let masks = if write {
            &mut self.stencil_masks.write
        } else {
            &mut self.stencil_masks.value
        };
        if face != gl::BACK {
            masks[0] = mask;
        }
        if face != gl::FRONT {
            masks[1] = mask;
        }
        Ok(())
    }
}
