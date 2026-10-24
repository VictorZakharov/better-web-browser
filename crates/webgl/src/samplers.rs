//! Sampler state belongs to opaque context objects and independent texture units.
use super::{ApiVersion, Command, Kind, Result, WebGl, gl};
use serde_json::{Value, json};

impl WebGl {
    pub(super) fn sampler_command(&mut self, c: &Command) -> Result<Value> {
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        if c.op == "createSampler" {
            let mut name = 0;
            unsafe {
                (self
                    .core
                    .as_ref()
                    .ok_or(gl::INVALID_OPERATION)?
                    .gen_samplers)(1, &mut name)
            };
            self.driver_result()?;
            return match self.objects.insert(Kind::Sampler, name) {
                Ok(id) => Ok(json!(id)),
                Err(error) => {
                    unsafe { super::extensions::delete_sampler(name) };
                    Err(error)
                }
            };
        }
        if c.op == "isSampler" {
            return Ok(json!(self.objects.get(c.u(0)?, Kind::Sampler).is_ok()));
        }
        if c.op == "bindSampler" {
            let unit = c.u(0)? as usize;
            if unit >= self.samplers.len() {
                return Err(gl::INVALID_VALUE);
            }
            let id = c.u(1)?;
            let native = self.objects.name(id, Kind::Sampler)?;
            unsafe {
                (self
                    .core
                    .as_ref()
                    .ok_or(gl::INVALID_OPERATION)?
                    .bind_sampler)(unit as u32, native)
            };
            self.driver_result()?;
            self.samplers[unit] = id;
            return Ok(Value::Null);
        }
        let id = c.u(0)?;
        if c.op == "deleteSampler" && id == 0 {
            return Ok(Value::Null);
        }
        let name = self.objects.get(id, Kind::Sampler)?.native;
        if c.op == "deleteSampler" {
            let function = self
                .core
                .as_ref()
                .ok_or(gl::INVALID_OPERATION)?
                .bind_sampler;
            for (unit, binding) in self.samplers.iter_mut().enumerate() {
                if *binding == id {
                    unsafe { function(unit as u32, 0) };
                    *binding = 0;
                }
            }
            self.objects.delete(id, Kind::Sampler)?;
            self.driver_result()?;
            return Ok(Value::Null);
        }
        let pname = c.u(1)?;
        let anisotropy = pname == 0x84fe
            && self
                .extensions
                .textures
                .enabled(super::texture_capabilities::TextureCapability::Anisotropy);
        if ![
            gl::TEXTURE_MIN_FILTER,
            gl::TEXTURE_MAG_FILTER,
            gl::TEXTURE_WRAP_S,
            gl::TEXTURE_WRAP_T,
            0x8072,
            0x813a,
            0x813b,
            0x884c,
            0x884d,
        ]
        .contains(&pname)
            && !anisotropy
        {
            return Err(gl::INVALID_ENUM);
        }
        let core = self.core.as_ref().ok_or(gl::INVALID_OPERATION)?;
        match c.op.as_str() {
            "samplerParameteri" => unsafe { (core.sampler_integer)(name, pname, c.n(2)?) },
            "samplerParameterf" => unsafe { (core.sampler_float)(name, pname, c.float(0)?) },
            "getSamplerParameter" => {
                let value = if [0x813a, 0x813b].contains(&pname) || anisotropy {
                    let mut value = 0.;
                    unsafe { (core.sampler_float_query)(name, pname, &mut value) };
                    super::float_values::encode(value)
                } else {
                    let mut value = 0;
                    unsafe { (core.sampler_integer_query)(name, pname, &mut value) };
                    json!(value)
                };
                self.driver_result()?;
                return Ok(value);
            }
            _ => return Err(gl::INVALID_OPERATION),
        }
        self.driver_result()?;
        Ok(Value::Null)
    }
}
