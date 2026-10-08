//! Native errors feed the context's bounded, deduplicated WebGL sticky set.
use super::{Result, WebGl, gl};

impl WebGl {
    pub(super) fn error(&mut self, error: u32) {
        if error != gl::NO_ERROR && !self.errors.contains(&error) && self.errors.len() < 8 {
            self.errors.push_back(error);
        }
    }

    pub(super) fn driver_result(&mut self) -> Result<()> {
        // Preserve earlier errors rather than replacing the per-context set.
        let mut first = None;
        for _ in 0..8 {
            let error = unsafe { gl::GetError() };
            if error == gl::NO_ERROR {
                break;
            }
            first.get_or_insert(error);
            self.error(error);
        }
        first.map_or(Ok(()), Err)
    }
}
