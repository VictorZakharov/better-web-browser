//! Renderer-owned GLES2 contexts with a WebGL compatibility/zero-initialization boundary.
//! ANGLE owns GLSL parsing, translation and driver calls. Browser handles never expose pointers.
use mozangle::gles::ffi as gl;
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};

mod adapter_selection;
mod api_version;
#[cfg(test)]
mod api_version_tests;
#[cfg(test)]
mod array_copy_boundary_tests;
mod attribute_reflection;
mod attribute_validation;
mod backend_policy;
#[cfg(test)]
mod buffer_mirror_tests;
mod buffer_retirement;
#[cfg(test)]
mod buffer_retirement_tests;
mod buffer_upload;
mod buffers;
mod command_batch;
pub(crate) mod numeric_packet;
pub(crate) use command_batch::{MAX_NUMERIC_VALUES, NumericCommand};
#[cfg(test)]
mod canvas_snapshot_tests;
#[cfg(test)]
mod command_batch_tests;
#[cfg(test)]
mod command_batch_uniform_tests;
#[cfg(test)]
mod command_numeric_tests;
mod commands;
mod compiler_events;
mod compiler_policy;
mod compiler_retirement;
#[cfg(test)]
mod compiler_retirement_tests;
mod compiler_workers;
#[cfg(test)]
mod compressed_buffer_tests;
mod compressed_capabilities;
#[cfg(test)]
mod compressed_core_tests;
#[cfg(test)]
mod compressed_core_validation_tests;
mod compressed_formats;
mod compressed_initialization;
mod compressed_storage;
#[cfg(test)]
mod compressed_texture_tests;
mod compressed_textures;
#[cfg(test)]
mod compressed_validation_tests;
mod compressed_volumes;
mod construction;
mod context;
mod context_state;
use context_state::WebGl;
mod copy_texture;
mod copy_texture_conversion;
mod core_attachment_queries;
#[cfg(test)]
mod core_attachment_tests;
mod core_attachments;
#[cfg(test)]
mod core_buffer_tests;
mod core_buffers;
#[cfg(test)]
mod core_color_admission_tests;
#[cfg(test)]
mod core_copy_boundary_tests;
mod core_copy_conversion;
mod core_copy_texture;
#[cfg(test)]
mod core_copy_texture_tests;
#[cfg(test)]
mod core_draw_tests;
mod core_draws;
mod core_entries;
#[cfg(test)]
mod core_extension_tests;
mod core_extensions;
#[cfg(test)]
mod core_framebuffer_tests;
mod core_framebuffers;
#[cfg(test)]
mod core_parameter_tests;
mod core_parameters;
mod core_renderbuffers;
#[cfg(test)]
mod core_texture_format_tests;
mod core_texture_formats;
#[cfg(test)]
mod core_texture_mip_tests;
mod core_texture_mips;
#[cfg(test)]
mod core_texture_tests;
#[cfg(test)]
mod core_texture_value_tests;
mod core_textures;
#[cfg(test)]
mod core_uniform_tests;
mod core_uniforms;
mod creation_options;
mod default_attachment_queries;
#[cfg(test)]
mod default_attachment_tests;
mod depth_textures;
mod draw_buffers;
mod drawing_buffer_extent;
#[cfg(test)]
mod drawing_buffer_extent_tests;
mod error_state;
mod execution_profile;
mod extension_commands;
mod extension_draw_dispatch;
mod extensions;
mod float_values;
mod framebuffer_attachments;
mod framebuffer_completeness;
#[cfg(test)]
mod framebuffer_contract_tests;
mod framebuffer_guard;
#[cfg(test)]
mod framebuffer_guard_tests;
mod framebuffer_invalidation;
#[cfg(test)]
mod framebuffer_invalidation_pixel_tests;
#[cfg(test)]
mod framebuffer_invalidation_storage_tests;
#[cfg(test)]
mod framebuffer_invalidation_tests;
mod framebuffer_queries;
mod framebuffers;
mod image_uploads;
#[cfg(test)]
mod immutable_attachment_tests;
mod index_cache;
#[cfg(test)]
mod index_cache_native_tests;
mod index_ranges;
mod indexed_blend;
mod indexed_blend_entries;
#[cfg(test)]
mod indexed_range_admission_tests;
mod indexed_uniform_buffers;
mod instancing;
mod legacy_mip_allocations;
#[cfg(test)]
mod link_reflection_tests;
mod link_submission;
#[cfg(test)]
mod link_submission_tests;
mod multi_draw;
mod multi_draw_entries;
mod multisample;
#[cfg(test)]
mod multisample_tests;
#[cfg(test)]
mod normalized_texture_pixel_tests;
#[cfg(test)]
mod normalized_texture_tests;
mod normalized_textures;
mod object_deletion;
mod object_queries;
mod objects;
#[cfg(test)]
mod parallel_compile_tests;
#[cfg(test)]
mod parallel_lifetime_tests;
mod parameters;
mod pending_links;
mod pixel_buffer_guard;
#[cfg(test)]
mod pixel_buffer_tests;
mod pixel_buffers;
mod pixel_layout;
#[cfg(test)]
mod pixel_layout_tests;
mod pixel_readback;
mod pixel_store_guard;
#[cfg(test)]
mod pixel_transfer_tests;
mod pixel_transport;
mod presentation;
#[cfg(test)]
mod presentation_state_tests;
mod process_headroom;
mod queries;
#[cfg(test)]
mod query_object_tests;
mod query_objects;
#[cfg(test)]
mod read_pair_validation_tests;
mod readback_cache;
#[cfg(test)]
mod readback_cache_tests;
mod resize;
mod resource_budget;
mod resource_diagnostics;
#[cfg(test)]
mod same_size_resize_tests;
#[cfg(test)]
mod sampler_tests;
mod samplers;
mod session;
mod shader_commands;
#[cfg(test)]
mod shader_name_tests;
mod shader_names;
mod shader_queries;
#[cfg(test)]
mod shader_source_budget_tests;
mod shader_validation;
mod shader_validation_cache;
mod shader_validator_resources;
mod shader_validators;
mod stencil_masks;
mod surface;
mod surface_multisample;
#[cfg(test)]
mod surface_multisample_tests;
mod sync_entries;
#[cfg(test)]
mod sync_object_tests;
mod sync_objects;
mod sync_reply_cache;
mod task_completion;
#[cfg(test)]
mod task_completion_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod texture_allocation_tests;
mod texture_allocations;
mod texture_capabilities;
mod texture_color_space;
mod texture_dispatch;
mod texture_formats;
mod texture_sampling;
mod texture_targets;
mod textures;
#[cfg(test)]
mod transform_array_tests;
mod transform_buffers;
#[cfg(test)]
mod transform_builtin_tests;
mod transform_capacity;
#[cfg(test)]
mod transform_delete_tests;
mod transform_entries;
mod transform_feedback;
#[cfg(test)]
mod transform_feedback_tests;
mod transform_safety;
#[cfg(test)]
mod transform_validation_tests;
mod transform_varyings;
#[cfg(test)]
mod typed_framebuffer_tests;
mod typed_framebuffers;
mod uniform_block_queries;
#[cfg(test)]
mod uniform_block_tests;
#[cfg(test)]
mod uniform_block_validation_tests;
mod uniform_blocks;
#[cfg(test)]
mod uniform_metadata_tests;
mod uniform_queries;
mod uniform_reflection;
#[cfg(test)]
mod uniform_reflection_tests;
mod uniform_type;
#[cfg(test)]
mod uniform_validation_tests;
mod uniforms;
#[cfg(test)]
mod validation_tests;
mod vertex_arrays;
mod vertex_attribute_queries;
#[cfg(test)]
mod vertex_attribute_tests;
mod vertex_attributes;
#[cfg(test)]
mod vertex_id_boundary_tests;
#[cfg(test)]
mod volume_blit_tests;
mod volume_copy;
#[cfg(test)]
mod volume_copy_alias_tests;
mod volume_copy_blit;
mod volume_copy_conversion;
#[cfg(test)]
mod volume_copy_conversion_tests;
mod volume_copy_region;
mod volume_copy_staging;
#[cfg(test)]
mod volume_copy_tests;
#[cfg(test)]
mod volume_copy_transfer_tests;
mod volume_initialization;
#[cfg(test)]
mod volume_mip_tests;
#[cfg(test)]
mod volume_texture_tests;
mod volume_textures;
pub(crate) use pixel_transport::PixelReply;
pub(crate) use session::Contexts;

use api_version::ApiVersion;
use context::NativeContext;
use creation_options::Options;
use objects::{Kind, Objects};
use surface::Surface;

pub(crate) const MAX_CONTEXTS: usize = 8;
// Generated object names have bounded browser metadata even before they own
// storage. Asset-heavy applications legitimately exceed 1,024 names during
// loading; storage remains independently subject to the per-context/process
// byte ledgers. This is an admission limit, not an advertised GL capability.
const MAX_OBJECTS: usize = 8192;
// Finite HDR/MSAA capacity, additionally gated by measured process headroom.
// Object storage still reserves two copies; the selected Job cap stays finite.
#[cfg(test)]
const MAX_RESOURCE_BYTES: usize = 256 * 1024 * 1024;

fn resource_ceiling() -> usize {
    crate::renderer_budget::current().gpu_context_bytes()
}

fn owner_resource_ceiling() -> usize {
    crate::renderer_budget::current().gpu_owner_bytes()
}
const MAX_NATIVE_CONTEXTS: usize = 16;
const MAX_UPLOAD_BYTES: usize = 16 * 1024 * 1024;
const MAX_SHADER_BYTES: usize = 32 * 1024;
// Generated standards-compliant materials routinely exceed the log/reflection
// budget. Keep source admission independent, with bounded JSON escape expansion.
const MAX_SHADER_SOURCE_BYTES: usize = 256 * 1024;
const MAX_COMMAND_BYTES: usize = MAX_SHADER_SOURCE_BYTES * 6 + 4096;
const MAX_DRAW_VERTICES: u32 = 1_000_000;
type Result<T> = std::result::Result<T, u32>;

mod backend;
use backend::BackendContexts;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Command {
    op: String,
    #[serde(default)]
    i: Vec<i64>,
    #[serde(default, deserialize_with = "float_values::deserialize")]
    f: Vec<f64>,
    #[serde(default)]
    text: String,
}
impl Command {
    fn u(&self, index: usize) -> Result<u32> {
        self.i
            .get(index)
            .copied()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or(gl::INVALID_VALUE)
    }
    fn n(&self, index: usize) -> Result<i32> {
        self.i
            .get(index)
            .copied()
            .and_then(|n| i32::try_from(n).ok())
            .ok_or(gl::INVALID_VALUE)
    }
    fn float(&self, index: usize) -> Result<f32> {
        self.f
            .get(index)
            .copied()
            .map(|n| n as f32)
            .ok_or(gl::INVALID_VALUE)
    }
}
