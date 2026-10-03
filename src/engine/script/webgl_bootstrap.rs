// Shared WebGL bootstrap: Window and Worker must register the same APIs.
macro_rules! webgl_bootstrap {
    () => {
        concat!(
    include_str!("bootstrap/webgl_attributes.js"),
    include_str!("bootstrap/webgl_context.js"),
    include_str!("bootstrap/webgl_argument_brands.js"),
    include_str!("bootstrap/webgl_numeric_arguments.js"),
    include_str!("bootstrap/webgl_lifecycle.js"),
    include_str!("bootstrap/webgl_extensions.js"),
    include_str!("bootstrap/webgl_texture_extensions.js"),
    include_str!("bootstrap/webgl_vertex_arrays.js"),
    include_str!("bootstrap/webgl_methods.js"),
    include_str!("bootstrap/webgl_queries.js"),
    include_str!("bootstrap/webgl_textures.js"),
    include_str!("bootstrap/webgl_constants.js"),
        )
    };
}
pub(super) use webgl_bootstrap;
