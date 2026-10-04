//! Core GLES3 facilities do not require author WebGL1 extension objects.
use super::{core_entries::CoreEntries, extensions::Extensions};

impl Extensions {
    pub(super) fn admit_core(&mut self, core: &CoreEntries) {
        self.gen_arrays = Some(core.gen_arrays);
        self.bind_array = Some(core.bind_array);
        self.is_array = Some(core.is_array);
        self.arrays = Some(core.arrays);
        self.elements = Some(core.elements);
        self.divisor = Some(core.divisor);
        self.draw_buffers_entry = Some(core.draw_buffers);
        self.vertex_arrays = true;
        self.instancing = true;
        self.uint_indices = true;
        self.derivatives = true;
        self.frag_depth = true;
        self.texture_lod = true;
        self.draw_buffers = true;
        // These facilities are core, not extensions returned by getExtension.
        self.available_vertex_arrays = false;
        self.available_instancing = false;
        self.available_uint_indices = false;
        self.available_derivatives = false;
        self.available_frag_depth = false;
        self.available_texture_lod = false;
        self.available_draw_buffers = false;
    }
}
