//! Versioned texture binding slots never confuse volume and cube targets.
use super::{ApiVersion, Result, WebGl, gl};
pub(super) const VOLUME: u32 = 0x806f;
pub(super) const ARRAY: u32 = 0x8c1a;
pub(super) fn target(slot: usize) -> u32 {
    [gl::TEXTURE_2D, gl::TEXTURE_CUBE_MAP, VOLUME, ARRAY][slot]
}
impl WebGl {
    pub(super) fn texture_slot(&self, target: u32) -> Result<usize> {
        match target {
            VOLUME if self.options.api == ApiVersion::Two => Ok(2),
            ARRAY if self.options.api == ApiVersion::Two => Ok(3),
            _ => super::textures::texture_slot(target),
        }
    }
}
