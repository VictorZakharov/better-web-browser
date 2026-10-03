//! Closed browser extension negotiation; native availability never implies author enablement.
use super::texture_capabilities::TextureCapability;
use super::{Command, Result, WebGl, gl, json};
use serde_json::Value;
impl WebGl {
    pub(super) fn extension_command(&mut self, c: &Command) -> Result<Value> {
        match c.op.as_str() {
            "supportedExtensions" => {
                let mut names = Vec::new();
                for family in super::compressed_capabilities::Family::ALL {
                    if self.extensions.compressed.available(family) {
                        names.push(family.public_name());
                    }
                }
                if self.options.api == super::ApiVersion::Two
                    && self.extensions.available_core_color_float
                {
                    names.push("EXT_color_buffer_float");
                }
                if self.extensions.available_draw_buffers {
                    names.push("WEBGL_draw_buffers");
                }
                if self.extensions.available_instancing {
                    names.push("ANGLE_instanced_arrays");
                }
                if self.extensions.available_vertex_arrays {
                    names.push("OES_vertex_array_object");
                }
                if self.extensions.available_uint_indices {
                    names.push("OES_element_index_uint");
                }
                if self.extensions.available_derivatives {
                    names.push("OES_standard_derivatives");
                }
                if self.extensions.available_frag_depth {
                    names.push("EXT_frag_depth");
                }
                if self.extensions.available_texture_lod {
                    names.push("EXT_shader_texture_lod");
                }
                for capability in TextureCapability::ALL {
                    if self.extensions.textures.available(capability) {
                        names.push(capability.public_name());
                    }
                }
                Ok(json!(names))
            }
            "enableExtension" => {
                if let Some(family) = super::compressed_capabilities::Family::ALL
                    .into_iter()
                    .find(|family| family.public_name() == c.text)
                {
                    let enabled = self.extensions.enable_compressed(family);
                    self.driver_result()?;
                    return Ok(json!(enabled));
                }
                if c.text == "EXT_color_buffer_float" {
                    if self.options.api != super::ApiVersion::Two {
                        return Ok(json!(false));
                    }
                    self.extensions.core_color_float = self.extensions.enable_simple(
                        c"GL_EXT_color_buffer_float",
                        self.extensions.available_core_color_float,
                    );
                    self.driver_result()?;
                    return Ok(json!(self.extensions.core_color_float));
                }
                let texture = TextureCapability::ALL
                    .into_iter()
                    .find(|capability| capability.public_name() == c.text);
                let enabled = if let Some(capability) = texture {
                    self.extensions.enable_texture(capability)
                } else {
                    match c.text.as_str() {
                        "WEBGL_draw_buffers" => {
                            self.extensions.draw_buffers = self.extensions.enable_simple(
                                c"GL_EXT_draw_buffers",
                                self.extensions.available_draw_buffers,
                            );
                            self.extensions.draw_buffers
                        }
                        "ANGLE_instanced_arrays" => self.extensions.enable_instancing(),
                        "OES_vertex_array_object" => self.enable_vertex_arrays()?,
                        "OES_element_index_uint" => {
                            self.extensions.uint_indices = self.extensions.enable_simple(
                                c"GL_OES_element_index_uint",
                                self.extensions.available_uint_indices,
                            );
                            self.extensions.uint_indices
                        }
                        "OES_standard_derivatives" => {
                            self.extensions.derivatives = self.extensions.enable_simple(
                                c"GL_OES_standard_derivatives",
                                self.extensions.available_derivatives,
                            );
                            self.extensions.derivatives
                        }
                        "EXT_frag_depth" => {
                            self.extensions.frag_depth = self.extensions.enable_simple(
                                c"GL_EXT_frag_depth",
                                self.extensions.available_frag_depth,
                            );
                            self.extensions.frag_depth
                        }
                        "EXT_shader_texture_lod" => {
                            self.extensions.texture_lod = self.extensions.enable_simple(
                                c"GL_EXT_shader_texture_lod",
                                self.extensions.available_texture_lod,
                            );
                            self.extensions.texture_lod
                        }
                        _ => false,
                    }
                };
                self.driver_result()?;
                Ok(json!(enabled))
            }
            _ => Err(gl::INVALID_OPERATION),
        }
    }
}
