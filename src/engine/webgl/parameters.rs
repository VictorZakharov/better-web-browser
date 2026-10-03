//! Closed-size WebGL state queries; native pointers never cross the bridge.
use super::{Result, WebGl, gl, json};
use serde_json::Value;
impl WebGl {
    fn implementation_read_type(&mut self) -> Result<u32> {
        // State queries use INVALID_OPERATION for an incomplete read surface;
        // actual readPixels retains INVALID_FRAMEBUFFER_OPERATION.
        self.color_read_type().map_err(|error| {
            if error == gl::INVALID_FRAMEBUFFER_OPERATION {
                gl::INVALID_OPERATION
            } else {
                error
            }
        })
    }

    pub(super) fn parameter(&mut self, pname: u32) -> Result<Value> {
        if let Some(value) = self.core_parameter(pname)? {
            return Ok(value);
        }
        if let Some(value) = self.core_buffer_parameter(pname)? {
            return Ok(value);
        }
        if let Some(value) = self.draw_buffer_parameter(pname)? {
            return Ok(value);
        }
        if pname == 0x84ff {
            if !self
                .extensions
                .textures
                .enabled(super::texture_capabilities::TextureCapability::Anisotropy)
            {
                return Err(gl::INVALID_ENUM);
            }
            let mut value = 0.0;
            unsafe {
                gl::GetFloatv(pname, &mut value);
            }
            self.driver_result()?;
            return Ok(json!(value));
        }
        if let Some(mask) = self.stencil_masks.query(pname) {
            return Ok(json!(mask));
        }
        if pname == 0x8b8b {
            if !self.extensions.derivatives {
                return Err(gl::INVALID_ENUM);
            }
            let mut hint = 0;
            unsafe {
                gl::GetIntegerv(pname, &mut hint);
            }
            self.driver_result()?;
            return Ok(json!(hint));
        }
        if pname == 0x85b5 {
            if self.vertex_arrays.default_native == 0 {
                return Err(gl::INVALID_ENUM);
            }
            return Ok(json!(self.vertex_arrays.bound));
        }
        // Report the supported read pair for the currently bound color surface,
        // not a native optional packed format that the bridge cannot transport.
        if pname == 0x8b9b {
            self.implementation_read_type()?;
            return Ok(json!(gl::RGBA));
        }
        if pname == 0x8b9a {
            return Ok(json!(self.implementation_read_type()?));
        }
        if [
            gl::MAX_TEXTURE_SIZE,
            gl::MAX_CUBE_MAP_TEXTURE_SIZE,
            gl::MAX_RENDERBUFFER_SIZE,
        ]
        .contains(&pname)
        {
            let mut limit = 0;
            unsafe {
                gl::GetIntegerv(pname, &mut limit);
            }
            self.driver_result()?;
            return Ok(json!(limit.min(4096)));
        }
        if self.framebuffer == 0
            && ((pname == gl::ALPHA_BITS && !self.options.alpha)
                || (pname == gl::DEPTH_BITS && !self.options.depth)
                || (pname == gl::STENCIL_BITS && !self.options.stencil))
        {
            return Ok(json!(0));
        }
        let binding = match pname {
            gl::ARRAY_BUFFER_BINDING => Some(self.array_buffer),
            gl::ELEMENT_ARRAY_BUFFER_BINDING => Some(self.element_buffer),
            gl::CURRENT_PROGRAM => Some(self.program),
            gl::FRAMEBUFFER_BINDING => Some(self.framebuffer),
            gl::RENDERBUFFER_BINDING => Some(self.renderbuffer),
            gl::TEXTURE_BINDING_2D => Some(self.textures[self.texture_unit][0]),
            gl::TEXTURE_BINDING_CUBE_MAP => Some(self.textures[self.texture_unit][1]),
            _ => None,
        };
        if let Some(id) = binding {
            return Ok(if id == 0 { Value::Null } else { json!(id) });
        }
        match pname {
            gl::VENDOR => return Ok(json!("Breeze")),
            gl::RENDERER => return Ok(json!("ANGLE WebGL renderer")),
            gl::VERSION => return Ok(json!(self.options.api.version_string())),
            gl::SHADING_LANGUAGE_VERSION => {
                return Ok(json!(self.options.api.shader_version_string()));
            }
            gl::COMPRESSED_TEXTURE_FORMATS => return Ok(json!([])),
            gl::BLEND
            | gl::CULL_FACE
            | gl::DEPTH_TEST
            | gl::DITHER
            | gl::POLYGON_OFFSET_FILL
            | gl::SAMPLE_ALPHA_TO_COVERAGE
            | gl::SAMPLE_COVERAGE
            | gl::SCISSOR_TEST
            | gl::STENCIL_TEST => return Ok(json!(unsafe { gl::IsEnabled(pname) != 0 })),
            _ => {}
        }
        let floats = match pname {
            gl::COLOR_CLEAR_VALUE | gl::BLEND_COLOR => 4,
            gl::DEPTH_RANGE | gl::ALIASED_LINE_WIDTH_RANGE | gl::ALIASED_POINT_SIZE_RANGE => 2,
            gl::DEPTH_CLEAR_VALUE
            | gl::LINE_WIDTH
            | gl::POLYGON_OFFSET_FACTOR
            | gl::POLYGON_OFFSET_UNITS
            | gl::SAMPLE_COVERAGE_VALUE => 1,
            _ => 0,
        };
        if floats != 0 {
            let mut values = [0.0f32; 4];
            unsafe {
                gl::GetFloatv(pname, values.as_mut_ptr());
            }
            self.driver_result()?;
            return Ok(if floats == 1 {
                super::float_values::encode(values[0])
            } else {
                json!(
                    values[..floats]
                        .iter()
                        .copied()
                        .map(super::float_values::encode)
                        .collect::<Vec<_>>()
                )
            });
        }
        let count = match pname {
            gl::VIEWPORT | gl::SCISSOR_BOX | gl::COLOR_WRITEMASK => 4,
            gl::MAX_VIEWPORT_DIMS => 2,
            gl::ACTIVE_TEXTURE
            | gl::MAX_TEXTURE_SIZE
            | gl::MAX_CUBE_MAP_TEXTURE_SIZE
            | gl::MAX_RENDERBUFFER_SIZE
            | gl::MAX_VERTEX_ATTRIBS
            | gl::MAX_COMBINED_TEXTURE_IMAGE_UNITS
            | gl::MAX_VERTEX_TEXTURE_IMAGE_UNITS
            | gl::MAX_TEXTURE_IMAGE_UNITS
            | gl::MAX_VERTEX_UNIFORM_VECTORS
            | gl::MAX_FRAGMENT_UNIFORM_VECTORS
            | gl::MAX_VARYING_VECTORS
            | gl::RED_BITS
            | gl::GREEN_BITS
            | gl::BLUE_BITS
            | gl::ALPHA_BITS
            | gl::DEPTH_BITS
            | gl::STENCIL_BITS
            | gl::SUBPIXEL_BITS
            | gl::PACK_ALIGNMENT
            | gl::UNPACK_ALIGNMENT
            | gl::CULL_FACE_MODE
            | gl::FRONT_FACE
            | gl::DEPTH_FUNC
            | gl::DEPTH_WRITEMASK
            | gl::BLEND_SRC_RGB
            | gl::BLEND_SRC_ALPHA
            | gl::BLEND_DST_RGB
            | gl::BLEND_DST_ALPHA
            | gl::BLEND_EQUATION_RGB
            | gl::BLEND_EQUATION_ALPHA
            | gl::STENCIL_CLEAR_VALUE
            | gl::STENCIL_FUNC
            | gl::STENCIL_REF
            | gl::STENCIL_VALUE_MASK
            | gl::STENCIL_WRITEMASK
            | gl::STENCIL_FAIL
            | gl::STENCIL_PASS_DEPTH_FAIL
            | gl::STENCIL_PASS_DEPTH_PASS
            | gl::STENCIL_BACK_FUNC
            | gl::STENCIL_BACK_REF
            | gl::STENCIL_BACK_VALUE_MASK
            | gl::STENCIL_BACK_WRITEMASK
            | gl::STENCIL_BACK_FAIL
            | gl::STENCIL_BACK_PASS_DEPTH_FAIL
            | gl::STENCIL_BACK_PASS_DEPTH_PASS
            | gl::SAMPLE_BUFFERS
            | gl::SAMPLES
            | gl::SAMPLE_COVERAGE_INVERT
            | gl::GENERATE_MIPMAP_HINT => 1,
            _ => return Err(gl::INVALID_ENUM),
        };
        let mut values = [0i32; 4];
        unsafe {
            gl::GetIntegerv(pname, values.as_mut_ptr());
        }
        self.driver_result()?;
        Ok(if pname == gl::COLOR_WRITEMASK {
            json!(values.map(|n| n != 0))
        } else if [gl::DEPTH_WRITEMASK, gl::SAMPLE_COVERAGE_INVERT].contains(&pname) {
            json!(values[0] != 0)
        } else if [
            gl::STENCIL_VALUE_MASK,
            gl::STENCIL_WRITEMASK,
            gl::STENCIL_BACK_VALUE_MASK,
            gl::STENCIL_BACK_WRITEMASK,
        ]
        .contains(&pname)
        {
            json!(values[0] as u32)
        } else if count == 1 {
            json!(values[0])
        } else {
            json!(&values[..count])
        })
    }
}
