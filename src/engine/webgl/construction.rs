//! Provider version and private surface admission belong to context construction.
use super::*;

impl WebGl {
    pub(super) fn new(
        width: u32,
        height: u32,
        options: Options,
    ) -> std::result::Result<Self, String> {
        let native = NativeContext::for_api(options.api)?;
        let core = if options.api == ApiVersion::Two {
            Some(core_entries::CoreEntries::load()?)
        } else {
            None
        };
        let surface = Surface::new(width, height, options)?;
        unsafe {
            gl::ClearColor(0.0, 0.0, 0.0, 0.0);
        }
        let mut count = 0;
        let mut units = 0;
        // SAFETY: a current GLES2 context and writable scalar out-parameters.
        unsafe {
            gl::GetIntegerv(gl::MAX_VERTEX_ATTRIBS, &mut count);
            gl::GetIntegerv(gl::MAX_COMBINED_TEXTURE_IMAGE_UNITS, &mut units);
        }
        if !(8..=32).contains(&count) || !(8..=64).contains(&units) {
            return Err("ANGLE resource limits are outside the admitted WebGL baseline".into());
        }
        let mut context = Self {
            native,
            core,
            core_buffer_bindings: HashMap::new(),
            surface,
            objects: Objects::new(options.api),
            default_draw_buffer: gl::BACK,
            errors: VecDeque::new(),
            options,
            stencil_masks: stencil_masks::StencilMasks::default(),
            resource_bytes: width as usize
                * height as usize
                * if options.depth || options.stencil {
                    8
                } else {
                    4
                },
            resource_limit: MAX_RESOURCE_BYTES,
            array_buffer: 0,
            element_buffer: 0,
            program: 0,
            attributes: vec![buffers::Attribute::default(); count as usize],
            attribute_values: vec![vertex_attributes::ValueKind::Float; count as usize],
            extensions: extensions::Extensions::new(),
            vertex_arrays: vertex_arrays::VertexArrays::default(),
            framebuffer: 0,
            read_framebuffer: 0,
            default_read_buffer: gl::BACK,
            renderbuffer: 0,
            texture_unit: 0,
            textures: vec![[0; 4]; units as usize],
        };
        if let Some(core) = &context.core {
            context.extensions.admit_core(core);
            context
                .enable_vertex_arrays()
                .map_err(|error| format!("Initialize GLES3 vertex state: GL {error:#x}"))?;
            // WebGL2 always enables fixed-index primitive restart, not an author capability.
            unsafe {
                gl::Enable(0x8d69);
            }
            context
                .driver_result()
                .map_err(|error| format!("Initialize WebGL2 core state: GL {error:#x}"))?;
        }
        Ok(context)
    }
}
