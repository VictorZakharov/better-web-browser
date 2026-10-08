//! Read-only attribution includes objects awaiting their last container release.
use super::{Kind, Objects};

impl Objects {
    pub(in crate::engine::webgl) fn storage_summary(&self) -> (usize, usize, [usize; 4]) {
        let mut capacities = [0; 4];
        let mut pending = 0;
        for object in self.entries.values() {
            pending += usize::from(object.pending_delete);
            let index = match object.kind {
                Kind::Buffer => 0,
                Kind::Texture => 1,
                Kind::Renderbuffer => 2,
                Kind::Shader => 3,
                _ => continue,
            };
            // Admission bounds the sum well below usize::MAX.
            capacities[index] += object.capacity;
        }
        (self.entries.len(), pending, capacities)
    }
}
