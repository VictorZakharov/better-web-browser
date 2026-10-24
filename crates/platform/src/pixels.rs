//! RGBA8 associated-alpha byte arithmetic shared by codecs, Canvas and WebGL.
mod alpha;
mod premultiply;

pub use alpha::unpremultiply_rgba;
pub use premultiply::rgba_to_bgra;
