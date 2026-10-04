//! Provider version and private surface admission belong to context construction.
use super::*;

impl WebGl {
    pub(super) fn new(
        width: u32,
        height: u32,
        options: Options,
    ) -> std::result::Result<Self, String> {
        // Unit tests retain the same real WARP provider on every machine.
        // Release browser/harness runs exercise production hardware selection.
        let software_only = cfg!(test);
        let preferred = if software_only {
            None
        } else {
            adapter_selection::preferred(options.power_preference)
        };
        backend_policy::admit(options, software_only, preferred, |backend| {
            Self::new_with_backend(width, height, options, backend)
        })
    }

    fn new_with_backend(
        width: u32,
        height: u32,
        options: Options,
        backend: backend_policy::Backend,
    ) -> std::result::Result<Self, String> {
        let native = NativeContext::for_backend(options.api, backend)?;
        let core = if options.api == ApiVersion::Two {
            Some(core_entries::CoreEntries::load()?)
        } else {
            None
        };
        let surface = Surface::new(width, height, options, core.as_ref())?;
        let surface_bytes = surface.bytes();
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
        if !(8..=32).contains(&count) || !(8..=256).contains(&units) {
            return Err("ANGLE resource limits are outside the admitted WebGL baseline".into());
        }
        // Browser-owned binding arrays remain bounded independently of a
        // hardware provider's larger combined-unit capacity. Report this same
        // admitted limit to authors and the shader validator, not native 128/192.
        let units = units.min(64);
        let mut context = Self {
            native,
            core,
            core_buffer_bindings: HashMap::new(),
            surface,
            readback_cache: readback_cache::Cache::default(),
            objects: Objects::new(options.api),
            default_draw_buffer: gl::BACK,
            errors: VecDeque::new(),
            options,
            stencil_masks: stencil_masks::StencilMasks::default(),
            resource_bytes: surface_bytes,
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
            samplers: vec![0; units as usize],
            indexed_uniforms: indexed_uniform_buffers::Bindings::new(options.api)?,
            query_objects: query_objects::State::default(),
            sync_objects: sync_objects::State::default(),
            transform_feedback: transform_feedback::State::new(options.api)?,
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
impl Drop for WebGl {
    fn drop(&mut self) {
        if self.native.make_current().is_ok() {
            if self.vertex_arrays.default_native != 0 {
                unsafe {
                    extensions::delete_vertex_array(
                        self.vertex_arrays.default_native,
                        self.options.api,
                    );
                }
            }
            self.objects.delete_all();
            if let Some(core) = &self.core {
                self.sync_objects.destroy(&core.sync);
            }
            self.surface.destroy();
        }
        self.native.destroy();
    }
}
