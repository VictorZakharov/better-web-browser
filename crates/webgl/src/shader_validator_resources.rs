//! Native immutable limits plus the current explicit WebGL extension environment.
use super::{ApiVersion, WebGl, gl};
use mozangle::shaders::BuiltInResources;

pub(super) fn current(context: &WebGl) -> BuiltInResources {
    let mut range = [0; 2];
    let mut precision = 0;
    unsafe {
        gl::GetShaderPrecisionFormat(
            gl::FRAGMENT_SHADER,
            gl::HIGH_FLOAT,
            range.as_mut_ptr(),
            &mut precision,
        );
    }
    let mut resources = BuiltInResources {
        MaxVertexAttribs: limit(gl::MAX_VERTEX_ATTRIBS),
        MaxVertexUniformVectors: limit(gl::MAX_VERTEX_UNIFORM_VECTORS),
        MaxVaryingVectors: limit(gl::MAX_VARYING_VECTORS),
        MaxVertexTextureImageUnits: limit(gl::MAX_VERTEX_TEXTURE_IMAGE_UNITS),
        MaxCombinedTextureImageUnits: context.textures.len() as i32,
        MaxTextureImageUnits: limit(gl::MAX_TEXTURE_IMAGE_UNITS),
        MaxFragmentUniformVectors: limit(gl::MAX_FRAGMENT_UNIFORM_VECTORS),
        OES_standard_derivatives: i32::from(context.extensions.derivatives),
        EXT_frag_depth: i32::from(context.extensions.frag_depth),
        EXT_shader_texture_lod: i32::from(context.extensions.texture_lod),
        EXT_draw_buffers: i32::from(context.extensions.draw_buffers),
        ANGLE_multi_draw: i32::from(context.extensions.multi_draw),
        MaxDrawBuffers: if context.extensions.draw_buffers {
            context.extensions.max_draw_buffers as i32
        } else {
            1
        },
        FragmentPrecisionHigh: i32::from(precision > 0),
        HashFunction: None,
        ..BuiltInResources::default()
    };
    if context.options.api == ApiVersion::Two {
        // ESSL300 uses scalar native capabilities, not WebGL1 extension defaults.
        resources.MaxVertexOutputVectors = limit(0x9122) / 4;
        resources.MaxFragmentInputVectors = limit(0x9125) / 4;
        resources.MinProgramTexelOffset = limit(0x8904);
        resources.MaxProgramTexelOffset = limit(0x8905);
        resources.MaxFragmentUniformBlocks = limit(0x8a2d);
        resources.MaxVertexUniformBlocks = limit(0x8a2b);
        resources.MaxDrawBuffers = limit(0x8824);
    }
    resources
}

fn limit(pname: u32) -> i32 {
    let mut value = 0;
    // SAFETY: closed scalar queries on the already-current GPU owner context.
    unsafe { gl::GetIntegerv(pname, &mut value) };
    value
}
