//! Shader and program commands: typed ownership, linkage and public queries.
use super::{Command, Kind, MAX_SHADER_BYTES, MAX_SHADER_SOURCE_BYTES, Result, WebGl, gl, json};
use serde_json::Value;
use std::{ffi::CString, ptr};

impl WebGl {
    pub(super) fn shader_command(&mut self, c: &Command) -> Result<Value> {
        match c.op.as_str() {
            "shaderSource" => {
                let shader = self.objects.get(c.u(0)?, Kind::Shader)?.native;
                if c.text.len() > MAX_SHADER_SOURCE_BYTES || c.text.contains('\0') {
                    return Err(gl::INVALID_VALUE);
                }
                let previous = self.objects.get(c.u(0)?, Kind::Shader)?.capacity;
                self.charge(previous, c.text.len() + MAX_SHADER_BYTES)?;
                let object = self.objects.get_mut(c.u(0)?, Kind::Shader)?;
                object.bytes = c.text.as_bytes().to_vec();
                object.capacity = previous.max(c.text.len() + MAX_SHADER_BYTES);
                let source = CString::new(c.text.as_str()).map_err(|_| gl::INVALID_VALUE)?;
                let pointer = source.as_ptr();
                unsafe {
                    gl::ShaderSource(shader, 1, &pointer, ptr::null());
                }
            }
            "compileShader" => {
                let shader = self.objects.get(c.u(0)?, Kind::Shader)?.native;
                let source =
                    String::from_utf8_lossy(&self.objects.get(c.u(0)?, Kind::Shader)?.bytes)
                        .into_owned();
                let mut kind = 0;
                unsafe {
                    gl::GetShaderiv(shader, gl::SHADER_TYPE, &mut kind);
                }
                let translated = self.translate_shader(kind as u32, &source);
                let previous = self.objects.get(c.u(0)?, Kind::Shader)?.capacity;
                let translated_bytes = translated.as_ref().map(String::len).unwrap_or(0);
                let capacity = source.len() + MAX_SHADER_BYTES + translated_bytes;
                self.charge(previous, capacity)?;
                let object = self.objects.get_mut(c.u(0)?, Kind::Shader)?;
                object.capacity = previous.max(capacity);
                let (driver_source, log) = match translated {
                    Ok(source) => (source, String::new()),
                    // Compile invalid ESSL as well, so native linkage cannot reuse
                    // an earlier successful shader after validation has failed.
                    Err(log) => ("!".to_owned(), log),
                };
                self.objects.get_mut(c.u(0)?, Kind::Shader)?.shader_log = log;
                let source = CString::new(driver_source).map_err(|_| gl::INVALID_VALUE)?;
                let pointer = source.as_ptr();
                unsafe {
                    gl::ShaderSource(shader, 1, &pointer, ptr::null());
                    gl::CompileShader(shader);
                }
            }
            "attachShader" | "detachShader" => {
                let program_id = c.u(0)?;
                let shader_id = c.u(1)?;
                let program = self.objects.get(program_id, Kind::Program)?.native;
                let shader = self.objects.get(shader_id, Kind::Shader)?.native;
                unsafe {
                    if c.op == "attachShader" {
                        gl::AttachShader(program, shader);
                    } else {
                        gl::DetachShader(program, shader);
                    }
                }
                self.driver_result()?;
                if c.op == "attachShader" {
                    self.objects.attach(program_id, shader_id)?;
                } else {
                    self.objects.detach(program_id, shader_id)?;
                }
            }
            "linkProgram" | "validateProgram" => {
                if c.op == "linkProgram" {
                    self.validate_transform_program_link(c.u(0)?)?;
                }
                let object = self.objects.get_mut(c.u(0)?, Kind::Program)?;
                if c.op == "linkProgram" {
                    object.generation =
                        object.generation.checked_add(1).ok_or(gl::OUT_OF_MEMORY)?;
                }
                let program = object.native;
                unsafe {
                    if c.op == "linkProgram" {
                        gl::LinkProgram(program);
                    } else {
                        gl::ValidateProgram(program);
                    }
                }
            }
            "useProgram" => {
                let feedback = &self.transform_feedback.records[&self.transform_feedback.bound];
                if feedback.active && !feedback.paused {
                    return Err(gl::INVALID_OPERATION);
                }
                let id = c.u(0)?;
                let program = self.objects.name(id, Kind::Program)?;
                unsafe {
                    gl::UseProgram(program);
                }
                self.driver_result()?;
                self.objects.switch_program(self.program, id)?;
                self.program = id;
            }
            "getShaderParameter" | "getProgramParameter" => {
                let shader = c.op == "getShaderParameter";
                let pname = c.u(1)?;
                let allowed = if shader {
                    [gl::SHADER_TYPE, gl::DELETE_STATUS, gl::COMPILE_STATUS].contains(&pname)
                } else {
                    [
                        gl::DELETE_STATUS,
                        gl::LINK_STATUS,
                        gl::VALIDATE_STATUS,
                        gl::ATTACHED_SHADERS,
                        gl::ACTIVE_ATTRIBUTES,
                        gl::ACTIVE_UNIFORMS,
                    ]
                    .contains(&pname)
                };
                let core_blocks = !shader
                    && self.options.api == super::ApiVersion::Two
                    && [0x8a36, 0x8c7f, 0x8c83].contains(&pname);
                if !allowed && !core_blocks {
                    return Err(gl::INVALID_ENUM);
                }
                let object = self
                    .objects
                    .get(c.u(0)?, if shader { Kind::Shader } else { Kind::Program })?;
                let mut value = 0;
                unsafe {
                    if shader {
                        gl::GetShaderiv(object.native, pname, &mut value);
                    } else {
                        gl::GetProgramiv(object.native, pname, &mut value);
                    }
                }
                self.driver_result()?;
                return Ok(
                    if [
                        gl::DELETE_STATUS,
                        gl::COMPILE_STATUS,
                        gl::LINK_STATUS,
                        gl::VALIDATE_STATUS,
                    ]
                    .contains(&pname)
                    {
                        json!(value != 0)
                    } else {
                        json!(value)
                    },
                );
            }
            "getShaderInfoLog" | "getProgramInfoLog" => return self.shader_log(c),
            "getAttribLocation" | "bindAttribLocation" => {
                let program = self.objects.get(c.u(0)?, Kind::Program)?.native;
                if c.text.len() > self.options.api.query_name_budget()
                    || !c.text.is_ascii()
                    || c.text.starts_with("gl_")
                {
                    return Err(gl::INVALID_VALUE);
                }
                let name = CString::new(super::shader_validation::driver_name(&c.text))
                    .map_err(|_| gl::INVALID_VALUE)?;
                if c.op == "getAttribLocation" {
                    return Ok(json!(unsafe {
                        gl::GetAttribLocation(program, name.as_ptr())
                    }));
                }
                let index = c.u(1)?;
                if index as usize >= self.attributes.len() {
                    return Err(gl::INVALID_VALUE);
                }
                unsafe {
                    gl::BindAttribLocation(program, index, name.as_ptr());
                }
            }
            _ => return Err(gl::INVALID_OPERATION),
        }
        self.driver_result()?;
        Ok(Value::Null)
    }
}
