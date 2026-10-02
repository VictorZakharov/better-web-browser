//! Strict command boundary. No arbitrary GL entry point or pointer-valued parameter is exposed.
use super::{Command, Kind, MAX_SHADER_BYTES, Result, WebGl, gl, json};
use serde_json::Value;
use std::{ffi::CString, ptr};

impl WebGl {
    pub(super) fn dispatch(&mut self, c: &Command, bytes: Option<&[u8]>) -> Result<Value> {
        match c.op.as_str() {
            "bridgeError" => {
                self.error(c.u(0)?);
                return Ok(Value::Null);
            }
            "presented" => self.presented(),
            "getError" => {
                let _ = self.driver_result();
                return Ok(json!(self.errors.pop_front().unwrap_or(gl::NO_ERROR)));
            }
            "createBuffer" | "createProgram" | "createShader" => {
                let (kind, name) = match c.op.as_str() {
                    "createBuffer" => {
                        let mut name = 0;
                        unsafe {
                            gl::GenBuffers(1, &mut name);
                        }
                        (Kind::Buffer, name)
                    }
                    "createProgram" => (Kind::Program, unsafe { gl::CreateProgram() }),
                    _ => {
                        let kind = c.u(0)?;
                        if ![gl::VERTEX_SHADER, gl::FRAGMENT_SHADER].contains(&kind) {
                            return Err(gl::INVALID_ENUM);
                        }
                        (Kind::Shader, unsafe { gl::CreateShader(kind) })
                    }
                };
                self.driver_result()?;
                match self.objects.insert(kind, name) {
                    Ok(id) => return Ok(json!(id)),
                    Err(error) => {
                        // Allocation of the public name can fail independently of driver allocation.
                        unsafe {
                            match kind {
                                Kind::Buffer => gl::DeleteBuffers(1, &name),
                                Kind::Shader => gl::DeleteShader(name),
                                Kind::Program => gl::DeleteProgram(name),
                                Kind::Uniform => unreachable!(),
                            }
                        }
                        return Err(error);
                    }
                }
            }
            "deleteBuffer" | "deleteShader" | "deleteProgram" => {
                let kind = match c.op.as_str() {
                    "deleteBuffer" => Kind::Buffer,
                    "deleteShader" => Kind::Shader,
                    _ => Kind::Program,
                };
                let id = c.u(0)?;
                if kind == Kind::Buffer {
                    if self.array_buffer == id {
                        self.array_buffer = 0;
                    }
                    if self.element_buffer == id {
                        self.element_buffer = 0;
                    }
                }
                self.objects.delete(id, kind)?;
            }
            "shaderSource" => {
                let shader = self.objects.get(c.u(0)?, Kind::Shader)?.native;
                if c.text.len() > MAX_SHADER_BYTES || c.text.contains('\0') {
                    return Err(gl::INVALID_VALUE);
                }
                let source = CString::new(c.text.as_str()).map_err(|_| gl::INVALID_VALUE)?;
                let pointer = source.as_ptr();
                unsafe {
                    gl::ShaderSource(shader, 1, &pointer, ptr::null());
                }
            }
            "compileShader" => {
                let shader = self.objects.get(c.u(0)?, Kind::Shader)?.native;
                unsafe {
                    gl::CompileShader(shader);
                }
            }
            "attachShader" | "detachShader" => {
                let program = self.objects.get(c.u(0)?, Kind::Program)?.native;
                let shader = self.objects.get(c.u(1)?, Kind::Shader)?.native;
                unsafe {
                    if c.op == "attachShader" {
                        gl::AttachShader(program, shader);
                    } else {
                        gl::DetachShader(program, shader);
                    }
                }
            }
            "linkProgram" | "validateProgram" => {
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
                let id = c.u(0)?;
                let program = self.objects.name(id, Kind::Program)?;
                unsafe {
                    gl::UseProgram(program);
                }
                self.driver_result()?;
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
                if !allowed {
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
            "getShaderInfoLog" | "getProgramInfoLog" => {
                let shader = c.op == "getShaderInfoLog";
                let object = self
                    .objects
                    .get(c.u(0)?, if shader { Kind::Shader } else { Kind::Program })?;
                let mut length = 0;
                unsafe {
                    if shader {
                        gl::GetShaderiv(object.native, gl::INFO_LOG_LENGTH, &mut length);
                    } else {
                        gl::GetProgramiv(object.native, gl::INFO_LOG_LENGTH, &mut length);
                    }
                }
                let mut data = vec![0u8; length.clamp(1, MAX_SHADER_BYTES as i32) as usize];
                let mut written = 0;
                unsafe {
                    if shader {
                        gl::GetShaderInfoLog(
                            object.native,
                            data.len() as i32,
                            &mut written,
                            data.as_mut_ptr().cast(),
                        );
                    } else {
                        gl::GetProgramInfoLog(
                            object.native,
                            data.len() as i32,
                            &mut written,
                            data.as_mut_ptr().cast(),
                        );
                    }
                }
                data.truncate(written.max(0) as usize);
                self.driver_result()?;
                return Ok(json!(String::from_utf8_lossy(&data)));
            }
            "getAttribLocation" | "bindAttribLocation" => {
                let program = self.objects.get(c.u(0)?, Kind::Program)?.native;
                if c.text.len() > 256 || c.text.starts_with("gl_") {
                    return Err(gl::INVALID_VALUE);
                }
                let name = CString::new(c.text.as_str()).map_err(|_| gl::INVALID_VALUE)?;
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
            "clearColor" => unsafe {
                gl::ClearColor(c.float(0)?, c.float(1)?, c.float(2)?, c.float(3)?);
            },
            "clearDepth" => unsafe {
                gl::ClearDepthf(c.float(0)?);
            },
            "clearStencil" => unsafe {
                gl::ClearStencil(c.n(0)?);
            },
            "clear" => {
                let mask = c.u(0)?;
                if mask & !(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT | gl::STENCIL_BUFFER_BIT)
                    != 0
                {
                    return Err(gl::INVALID_VALUE);
                }
                unsafe {
                    gl::Clear(mask);
                }
            }
            "viewport" | "scissor" => {
                let x = c.n(0)?;
                let y = c.n(1)?;
                let width = c.n(2)?;
                let height = c.n(3)?;
                if width < 0 || height < 0 {
                    return Err(gl::INVALID_VALUE);
                }
                unsafe {
                    if c.op == "viewport" {
                        gl::Viewport(x, y, width, height);
                    } else {
                        gl::Scissor(x, y, width, height);
                    }
                }
            }
            "enable" | "disable" => {
                let cap = c.u(0)?;
                if ![
                    gl::BLEND,
                    gl::CULL_FACE,
                    gl::DEPTH_TEST,
                    gl::DITHER,
                    gl::POLYGON_OFFSET_FILL,
                    gl::SAMPLE_ALPHA_TO_COVERAGE,
                    gl::SAMPLE_COVERAGE,
                    gl::SCISSOR_TEST,
                    gl::STENCIL_TEST,
                ]
                .contains(&cap)
                {
                    return Err(gl::INVALID_ENUM);
                }
                unsafe {
                    if c.op == "enable" {
                        gl::Enable(cap);
                    } else {
                        gl::Disable(cap);
                    }
                }
            }
            "colorMask" => unsafe {
                gl::ColorMask(
                    u8::from(c.u(0)? != 0),
                    u8::from(c.u(1)? != 0),
                    u8::from(c.u(2)? != 0),
                    u8::from(c.u(3)? != 0),
                );
            },
            "depthMask" => unsafe {
                gl::DepthMask(u8::from(c.u(0)? != 0));
            },
            "depthFunc" => unsafe {
                gl::DepthFunc(c.u(0)?);
            },
            "blendFunc" => unsafe {
                gl::BlendFunc(c.u(0)?, c.u(1)?);
            },
            "blendEquation" => unsafe {
                gl::BlendEquation(c.u(0)?);
            },
            "frontFace" => unsafe {
                gl::FrontFace(c.u(0)?);
            },
            "cullFace" => unsafe {
                gl::CullFace(c.u(0)?);
            },
            "flush" => unsafe {
                gl::Flush();
            },
            "finish" => unsafe {
                gl::Finish();
            },
            "bindBuffer"
            | "bufferData"
            | "bufferSubData"
            | "vertexAttribPointer"
            | "enableVertexAttribArray"
            | "disableVertexAttribArray"
            | "drawArrays"
            | "drawElements" => return self.buffer_command(c, bytes),
            "getUniformLocation" | "uniform1f" | "uniform2f" | "uniform3f" | "uniform4f"
            | "uniform1fv" | "uniform2fv" | "uniform3fv" | "uniform4fv" | "uniform1i"
            | "uniform2i" | "uniform3i" | "uniform4i" | "uniform1iv" | "uniform2iv"
            | "uniform3iv" | "uniform4iv" | "uniformMatrix2fv" | "uniformMatrix3fv"
            | "uniformMatrix4fv" => return self.uniform_command(c),
            _ => return Err(gl::INVALID_OPERATION),
        }
        self.driver_result()?;
        Ok(Value::Null)
    }
}
