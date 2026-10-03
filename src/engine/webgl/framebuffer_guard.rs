//! Private drawing and reading must preserve GLES3's independent bindings.
use super::{ApiVersion, gl};
pub(super) const READ: u32 = 0x8ca8;
pub(super) const DRAW: u32 = 0x8ca9;
pub(super) const READ_BINDING: u32 = 0x8caa;
pub(super) enum Direction {
    Read,
    Draw,
}
pub(super) struct FramebufferGuard {
    target: u32,
    previous: u32,
}
impl FramebufferGuard {
    pub(super) fn bind(api: ApiVersion, direction: Direction, framebuffer: u32) -> Self {
        let (target, pname) = match (api, direction) {
            (ApiVersion::Two, Direction::Read) => (READ, READ_BINDING),
            (ApiVersion::Two, Direction::Draw) => (DRAW, gl::FRAMEBUFFER_BINDING),
            _ => (gl::FRAMEBUFFER, gl::FRAMEBUFFER_BINDING),
        };
        let mut previous = 0;
        unsafe {
            gl::GetIntegerv(pname, &mut previous);
            gl::BindFramebuffer(target, framebuffer);
        }
        Self {
            target,
            previous: previous as u32,
        }
    }
}
impl Drop for FramebufferGuard {
    fn drop(&mut self) {
        unsafe {
            gl::BindFramebuffer(self.target, self.previous);
        }
    }
}
