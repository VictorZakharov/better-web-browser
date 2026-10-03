//! Strict command boundary. No arbitrary GL entry point or pointer-valued parameter is exposed.
use super::{Command, Kind, Result, WebGl, gl, json};
use serde_json::Value;

impl WebGl {
    pub(super) fn dispatch(&mut self, c: &Command, bytes: Option<&[u8]>) -> Result<Value> {
        match c.op.as_str() {
            "readPixelsToBuffer" => return self.read_pixels_to_buffer(c, bytes),
            "texImage2DFromBuffer" | "texSubImage2DFromBuffer" => {
                if self.options.api != super::ApiVersion::Two {
                    return Err(gl::INVALID_OPERATION);
                }
                return self.core_texture_upload(c, bytes);
            }
            "texImage3DFromBuffer" | "texSubImage3DFromBuffer" => {
                return self.volume_texture_command(c, bytes);
            }
            "getUniformIndices"
            | "getActiveUniforms"
            | "getUniformBlockIndex"
            | "getActiveUniformBlockParameter"
            | "getActiveUniformBlockName"
            | "uniformBlockBinding" => {
                return self.uniform_block_command(c);
            }
            "bindBufferBase" | "bindBufferRange" | "getIndexedParameter" => {
                return self.indexed_uniform_command(c);
            }
            "createSampler"
            | "bindSampler"
            | "deleteSampler"
            | "isSampler"
            | "samplerParameteri"
            | "samplerParameterf"
            | "getSamplerParameter" => {
                return self.sampler_command(c);
            }
            "clearBufferfv" | "clearBufferiv" | "clearBufferuiv" | "clearBufferfi" => {
                return self.typed_clear(c);
            }
            "renderbufferStorageMultisample" | "getInternalformatParameter" | "blitFramebuffer" => {
                return self.multisample_command(c);
            }
            "framebufferTextureLayer" => {
                self.core_attach_framebuffer(c)?;
                return Ok(Value::Null);
            }
            "readBuffer" => return self.core_read_buffer(c),
            "texStorage3D" | "texImage3D" | "texSubImage3D" => {
                return self.volume_texture_command(c, bytes);
            }
            "texStorage2D" => return self.core_texture_storage(c),
            "drawRangeElements" => return self.draw_range_elements(c),
            "vertexAttribIPointer" => return self.vertex_pointer(c),
            "vertexAttribI4i" | "vertexAttribI4iv" | "vertexAttribI4ui" | "vertexAttribI4uiv" => {
                return self.integer_attribute(c);
            }
            "createVertexArray" | "bindVertexArray" | "deleteVertexArray" | "isVertexArray" => {
                if self.options.api != super::ApiVersion::Two {
                    return Err(gl::INVALID_OPERATION);
                }
                return self.vertex_array_command(c);
            }
            "vertexAttribDivisor" | "drawArraysInstanced" | "drawElementsInstanced" => {
                if self.options.api != super::ApiVersion::Two {
                    return Err(gl::INVALID_OPERATION);
                }
                return self.instanced_command(c);
            }
            "drawBuffers" => {
                if self.options.api != super::ApiVersion::Two {
                    return Err(gl::INVALID_OPERATION);
                }
                return self.draw_buffers_command(c);
            }
            "uniform1ui" | "uniform2ui" | "uniform3ui" | "uniform4ui" | "uniform1uiv"
            | "uniform2uiv" | "uniform3uiv" | "uniform4uiv" | "uniformMatrix2x3fv"
            | "uniformMatrix2x4fv" | "uniformMatrix3x2fv" | "uniformMatrix3x4fv"
            | "uniformMatrix4x2fv" | "uniformMatrix4x3fv" => return self.core_uniform_command(c),
            "copyBufferSubData" | "getBufferSubData" => return self.core_buffer_command(c),
            "drawBuffersWEBGL" => return self.draw_buffers_command(c),
            "supportedExtensions" | "enableExtension" => return self.extension_command(c),
            "createVertexArrayOES"
            | "bindVertexArrayOES"
            | "deleteVertexArrayOES"
            | "isVertexArrayOES" => return self.vertex_array_command(c),
            "vertexAttribDivisorANGLE"
            | "drawArraysInstancedANGLE"
            | "drawElementsInstancedANGLE" => return self.instanced_command(c),
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
                                _ => unreachable!(),
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
                    return self.delete_buffer(id);
                }
                self.objects.delete(id, kind)?;
            }
            "shaderSource"
            | "compileShader"
            | "attachShader"
            | "detachShader"
            | "linkProgram"
            | "validateProgram"
            | "useProgram"
            | "getShaderParameter"
            | "getProgramParameter"
            | "getShaderInfoLog"
            | "getProgramInfoLog"
            | "getAttribLocation"
            | "bindAttribLocation" => return self.shader_command(c),
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
                self.validate_framebuffer()?;
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
                let rasterizer_discard =
                    self.options.api == super::ApiVersion::Two && cap == 0x8c89;
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
                    && !rasterizer_discard
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
            "createTexture" | "deleteTexture" | "bindTexture" | "activeTexture" | "texImage2D"
            | "texSubImage2D" | "texParameteri" | "texParameterf" | "generateMipmap"
            | "pixelStorei" | "getTexParameter" => return self.texture_command(c, bytes),
            "createFramebuffer"
            | "deleteFramebuffer"
            | "bindFramebuffer"
            | "createRenderbuffer"
            | "deleteRenderbuffer"
            | "bindRenderbuffer"
            | "renderbufferStorage"
            | "framebufferTexture2D"
            | "framebufferRenderbuffer"
            | "checkFramebufferStatus"
            | "getRenderbufferParameter"
            | "getFramebufferAttachmentParameter" => return self.framebuffer_command(c),
            "getParameter"
            | "isEnabled"
            | "getBufferParameter"
            | "readPixels"
            | "getActiveUniform"
            | "getActiveAttrib"
            | "getShaderPrecisionFormat"
            | "getShaderSource"
            | "getAttachedShaders"
            | "getVertexAttrib"
            | "getVertexAttribOffset"
            | "getUniform" => return self.query_command(c, bytes),
            "resize" => return self.resize(c),
            "copyTexImage2D" | "copyTexSubImage2D" => return self.copy_texture(c),
            "hint" => {
                if c.u(0)? != gl::GENERATE_MIPMAP_HINT
                    && !(c.u(0)? == 0x8b8b && self.extensions.derivatives)
                {
                    return Err(gl::INVALID_ENUM);
                }
                unsafe {
                    gl::Hint(c.u(0)?, c.u(1)?);
                }
            }
            "compressedTexImage2D" | "compressedTexSubImage2D" => return Err(gl::INVALID_ENUM),
            "isBuffer" | "isTexture" | "isFramebuffer" | "isRenderbuffer" | "isShader"
            | "isProgram" => return self.object_query(c),
            "vertexAttrib1f" | "vertexAttrib2f" | "vertexAttrib3f" | "vertexAttrib4f" => {
                let index = c.u(0)?;
                if index as usize >= self.attributes.len() {
                    return Err(gl::INVALID_VALUE);
                }
                let mut values = [0.0, 0.0, 0.0, 1.0];
                let count = c.op.as_bytes()[12] - b'0';
                for (index, value) in values.iter_mut().enumerate().take(usize::from(count)) {
                    *value = c.float(index)?;
                }
                unsafe {
                    gl::VertexAttrib4fv(index, values.as_ptr());
                }
                self.driver_result()?;
                self.attribute_values[index as usize] = super::vertex_attributes::ValueKind::Float;
            }
            "blendColor" => unsafe {
                gl::BlendColor(c.float(0)?, c.float(1)?, c.float(2)?, c.float(3)?);
            },
            "blendFuncSeparate" => unsafe {
                gl::BlendFuncSeparate(c.u(0)?, c.u(1)?, c.u(2)?, c.u(3)?);
            },
            "blendEquationSeparate" => unsafe {
                gl::BlendEquationSeparate(c.u(0)?, c.u(1)?);
            },
            "depthRange" => unsafe {
                gl::DepthRangef(c.float(0)?, c.float(1)?);
            },
            "polygonOffset" => unsafe {
                gl::PolygonOffset(c.float(0)?, c.float(1)?);
            },
            "sampleCoverage" => unsafe {
                gl::SampleCoverage(c.float(0)?, u8::from(c.u(0)? != 0));
            },
            "lineWidth" => unsafe {
                gl::LineWidth(c.float(0)?);
            },
            "stencilFunc" | "stencilFuncSeparate" | "stencilMask" | "stencilMaskSeparate" => {
                self.stencil_mask_command(c)?
            }
            "stencilOp" => unsafe {
                gl::StencilOp(c.u(0)?, c.u(1)?, c.u(2)?);
            },
            "stencilOpSeparate" => unsafe {
                gl::StencilOpSeparate(c.u(0)?, c.u(1)?, c.u(2)?, c.u(3)?);
            },
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
