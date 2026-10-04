//! Read-only GLES queries use a closed list with statically sized out-parameters.
use super::{Command, Kind, MAX_SHADER_BYTES, Result, WebGl, gl, json};
use serde_json::Value;
impl WebGl {
    pub(super) fn query_command(&mut self, c: &Command, input: Option<&[u8]>) -> Result<Value> {
        match c.op.as_str() {
            "getParameter" => self.parameter(c.u(0)?),
            "isEnabled" => {
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
                Ok(json!(unsafe { gl::IsEnabled(cap) != 0 }))
            }
            "getBufferParameter" => {
                let target = c.u(0)?;
                self.bound_buffer(target)?;
                let pname = c.u(1)?;
                if ![gl::BUFFER_SIZE, gl::BUFFER_USAGE].contains(&pname) {
                    return Err(gl::INVALID_ENUM);
                }
                let mut value = 0;
                unsafe {
                    gl::GetBufferParameteriv(target, pname, &mut value);
                }
                self.driver_result()?;
                Ok(json!(value))
            }
            "readPixels" => self.read_pixels(c, input).map(|bytes| json!(bytes)),
            "getActiveUniform" | "getActiveAttrib" => {
                let program = self.objects.get(c.u(0)?, Kind::Program)?.native;
                let index = c.u(1)?;
                let uniform = c.op == "getActiveUniform";
                let mut count = 0;
                unsafe {
                    gl::GetProgramiv(
                        program,
                        if uniform {
                            gl::ACTIVE_UNIFORMS
                        } else {
                            gl::ACTIVE_ATTRIBUTES
                        },
                        &mut count,
                    );
                }
                if index >= count.max(0) as u32 {
                    return Err(gl::INVALID_VALUE);
                }
                let mut name = vec![0u8; MAX_SHADER_BYTES];
                let (mut written, mut size, mut kind) = (0, 0, 0);
                unsafe {
                    if uniform {
                        gl::GetActiveUniform(
                            program,
                            index,
                            name.len() as i32,
                            &mut written,
                            &mut size,
                            &mut kind,
                            name.as_mut_ptr().cast(),
                        );
                    } else {
                        gl::GetActiveAttrib(
                            program,
                            index,
                            name.len() as i32,
                            &mut written,
                            &mut size,
                            &mut kind,
                            name.as_mut_ptr().cast(),
                        );
                    }
                }
                self.driver_result()?;
                name.truncate((written.max(0) as usize).min(name.len()));
                Ok(
                    json!({"name":super::shader_validation::public_name(&String::from_utf8_lossy(&name)), "size":size, "type":kind}),
                )
            }
            "getShaderPrecisionFormat" => {
                let shader = c.u(0)?;
                let precision = c.u(1)?;
                if ![gl::VERTEX_SHADER, gl::FRAGMENT_SHADER].contains(&shader)
                    || ![
                        gl::LOW_FLOAT,
                        gl::MEDIUM_FLOAT,
                        gl::HIGH_FLOAT,
                        gl::LOW_INT,
                        gl::MEDIUM_INT,
                        gl::HIGH_INT,
                    ]
                    .contains(&precision)
                {
                    return Err(gl::INVALID_ENUM);
                }
                let mut range = [0; 2];
                let mut value = 0;
                unsafe {
                    gl::GetShaderPrecisionFormat(shader, precision, range.as_mut_ptr(), &mut value);
                }
                self.driver_result()?;
                Ok(json!({"rangeMin":range[0], "rangeMax":range[1], "precision":value}))
            }
            "getShaderSource" => {
                let shader = self.objects.get(c.u(0)?, Kind::Shader)?;
                Ok(json!(String::from_utf8_lossy(&shader.bytes)))
            }
            "getAttachedShaders" => {
                let program = self.objects.get(c.u(0)?, Kind::Program)?.native;
                let mut shaders = [0; 2];
                let mut count = 0;
                unsafe {
                    gl::GetAttachedShaders(program, 2, &mut count, shaders.as_mut_ptr());
                }
                self.driver_result()?;
                Ok(json!(
                    shaders[..count.clamp(0, 2) as usize]
                        .iter()
                        .filter_map(|n| self.objects.public_name(*n, Kind::Shader))
                        .collect::<Vec<_>>()
                ))
            }
            "getVertexAttrib" | "getVertexAttribOffset" => self.vertex_attribute_query(c),
            "getUniform" => self.get_uniform(c),
            _ => Err(gl::INVALID_OPERATION),
        }
    }
}
