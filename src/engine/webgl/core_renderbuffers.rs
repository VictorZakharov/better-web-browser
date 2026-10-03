//! Core renderbuffer formats do not inherit WebGL1's unsized depth/stencil alias.
use super::{Command, Kind, Result, WebGl, gl};
use serde_json::Value;

pub(super) fn storage_bytes(format: u32) -> Result<usize> {
    if format == gl::STENCIL_INDEX8 {
        return Ok(4);
    }
    if ![
        0x8229, 0x822b, 0x8051, 0x8058, 0x8c43, 0x8d62, 0x8056, 0x8057, 0x8059, 0x8231, 0x8232,
        0x8233, 0x8234, 0x8235, 0x8236, 0x8237, 0x8238, 0x8239, 0x823a, 0x823b, 0x823c, 0x8d8e,
        0x8d7c, 0x8d88, 0x8d76, 0x8d82, 0x8d70, 0x906f, 0x81a5, 0x81a6, 0x8cac, 0x88f0, 0x8cad,
    ]
    .contains(&format)
    {
        return Err(gl::INVALID_ENUM);
    }
    Ok(super::core_texture_formats::storage(format)?.bytes)
}
impl WebGl {
    pub(super) fn core_renderbuffer_storage(&mut self, c: &Command) -> Result<Value> {
        if c.u(0)? != gl::RENDERBUFFER {
            return Err(gl::INVALID_ENUM);
        }
        let format = c.u(1)?;
        let native_format = if format == 0x84f9 { 0x88f0 } else { format };
        let bytes = storage_bytes(native_format)?;
        let (width, height) = (c.n(2)?, c.n(3)?);
        if !(0..=4096).contains(&width) || !(0..=4096).contains(&height) {
            return Err(gl::INVALID_VALUE);
        }
        self.objects.get(self.renderbuffer, Kind::Renderbuffer)?;
        self.charge(0, width as usize * height as usize * bytes)?;
        unsafe {
            gl::RenderbufferStorage(gl::RENDERBUFFER, native_format, width, height);
        }
        self.driver_result()?;
        self.objects
            .get_mut(self.renderbuffer, Kind::Renderbuffer)?
            .renderbuffer_format = native_format;
        Ok(Value::Null)
    }
}
