//! Read-only GLES queries use a closed list with statically sized out-parameters.
use super::{Command, Kind, MAX_SHADER_BYTES, MAX_UPLOAD_BYTES, Result, WebGl, gl, json};
use serde_json::Value;
impl WebGl {
    pub(super) fn query_command(&mut self, c: &Command, input: Option<&[u8]>) -> Result<Value> {
        match c.op.as_str() {
            "getParameter" => self.parameter(c.u(0)?),
            "isEnabled" => {
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
                Ok(json!(unsafe { gl::IsEnabled(cap) != 0 }))
            }
            "getBufferParameter" => {
                let target = c.u(0)?;
                if ![gl::ARRAY_BUFFER, gl::ELEMENT_ARRAY_BUFFER].contains(&target) {
                    return Err(gl::INVALID_ENUM);
                }
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
            "readPixels" => {
                let width = c.n(2)?;
                let height = c.n(3)?;
                if width < 0 || height < 0 {
                    return Err(gl::INVALID_VALUE);
                }
                if c.u(4)? != gl::RGBA || c.u(5)? != gl::UNSIGNED_BYTE {
                    return Err(gl::INVALID_OPERATION);
                }
                let mut alignment = 0;
                unsafe {
                    gl::GetIntegerv(gl::PACK_ALIGNMENT, &mut alignment);
                }
                let size = super::textures::pixel_size(
                    width as usize,
                    height as usize,
                    4,
                    alignment as usize,
                )?;
                if size > MAX_UPLOAD_BYTES {
                    return Err(gl::OUT_OF_MEMORY);
                }
                if (c.u(6)? as usize) < size {
                    return Err(gl::INVALID_OPERATION);
                }
                let mut bytes = match input {
                    Some(data) if data.len() >= size => data[..size].to_vec(),
                    Some(_) => return Err(gl::INVALID_OPERATION),
                    None => vec![0u8; size],
                };
                unsafe {
                    gl::ReadPixels(
                        c.n(0)?,
                        c.n(1)?,
                        width,
                        height,
                        gl::RGBA,
                        gl::UNSIGNED_BYTE,
                        bytes.as_mut_ptr().cast(),
                    );
                }
                self.driver_result()?;
                if self.framebuffer == 0 && !self.options.alpha {
                    let stride =
                        (width as usize * 4).div_ceil(alignment as usize) * alignment as usize;
                    let x = c.n(0)?;
                    let y = c.n(1)?;
                    for row in 0..height as usize {
                        for column in 0..width as usize {
                            let sx = i64::from(x) + column as i64;
                            let sy = i64::from(y) + row as i64;
                            if sx >= 0
                                && sy >= 0
                                && sx < i64::from(self.surface.width)
                                && sy < i64::from(self.surface.height)
                            {
                                bytes[row * stride + column * 4 + 3] = 255;
                            }
                        }
                    }
                }
                Ok(json!(bytes))
            }
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
            "getVertexAttrib" | "getVertexAttribOffset" => {
                let index = c.u(0)?;
                if index as usize >= self.attributes.len() {
                    return Err(gl::INVALID_VALUE);
                }
                let pname = c.u(1)?;
                if c.op == "getVertexAttrib" && pname == 0x88fe {
                    if !self.extensions.instancing {
                        return Err(gl::INVALID_ENUM);
                    }
                    return Ok(json!(self.attributes[index as usize].divisor));
                }
                if c.op == "getVertexAttribOffset" {
                    if pname != gl::VERTEX_ATTRIB_ARRAY_POINTER {
                        return Err(gl::INVALID_ENUM);
                    }
                    return Ok(json!(self.attributes[index as usize].offset));
                }
                if pname == gl::CURRENT_VERTEX_ATTRIB {
                    let mut values = [0.0f32; 4];
                    unsafe {
                        gl::GetVertexAttribfv(index, pname, values.as_mut_ptr());
                    }
                    return Ok(json!(values.map(super::float_values::encode)));
                }
                if ![
                    gl::VERTEX_ATTRIB_ARRAY_ENABLED,
                    gl::VERTEX_ATTRIB_ARRAY_SIZE,
                    gl::VERTEX_ATTRIB_ARRAY_STRIDE,
                    gl::VERTEX_ATTRIB_ARRAY_TYPE,
                    gl::VERTEX_ATTRIB_ARRAY_NORMALIZED,
                    gl::VERTEX_ATTRIB_ARRAY_BUFFER_BINDING,
                ]
                .contains(&pname)
                {
                    return Err(gl::INVALID_ENUM);
                }
                if pname == gl::VERTEX_ATTRIB_ARRAY_BUFFER_BINDING {
                    return Ok(json!(self.attributes[index as usize].buffer));
                }
                // Keep detached attributes' format/stride even when their native
                // array was rebuilt to release a deleted buffer safely in GLES2.
                let attribute = &self.attributes[index as usize];
                let component = match attribute.kind {
                    gl::BYTE | gl::UNSIGNED_BYTE => 1,
                    gl::SHORT | gl::UNSIGNED_SHORT => 2,
                    _ => 4,
                };
                match pname {
                    gl::VERTEX_ATTRIB_ARRAY_ENABLED => return Ok(json!(attribute.enabled)),
                    gl::VERTEX_ATTRIB_ARRAY_SIZE => return Ok(json!(attribute.size / component)),
                    gl::VERTEX_ATTRIB_ARRAY_STRIDE => return Ok(json!(attribute.stride)),
                    gl::VERTEX_ATTRIB_ARRAY_TYPE => return Ok(json!(attribute.kind)),
                    gl::VERTEX_ATTRIB_ARRAY_NORMALIZED => return Ok(json!(attribute.normalized)),
                    _ => {}
                }
                let mut value = 0;
                unsafe {
                    gl::GetVertexAttribiv(index, pname, &mut value);
                }
                self.driver_result()?;
                Ok(
                    if [
                        gl::VERTEX_ATTRIB_ARRAY_ENABLED,
                        gl::VERTEX_ATTRIB_ARRAY_NORMALIZED,
                    ]
                    .contains(&pname)
                    {
                        json!(value != 0)
                    } else {
                        json!(value)
                    },
                )
            }
            "getUniform" => self.get_uniform(c),
            _ => Err(gl::INVALID_OPERATION),
        }
    }
}
