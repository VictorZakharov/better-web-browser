//! Private bitmap reads and initialization ignore author row/skip state.
//! Restore every scalar on all exits, including allocation/driver failures.
use super::pixel_layout::Direction;
use super::{ApiVersion, gl};
pub(super) struct PixelStoreGuard(Vec<(u32, i32)>);
impl PixelStoreGuard {
    pub(super) fn tight(api: ApiVersion, direction: Direction) -> Self {
        let alignment = match direction {
            Direction::Pack => gl::PACK_ALIGNMENT,
            Direction::Unpack => gl::UNPACK_ALIGNMENT,
        };
        let extra: &[u32] = if api == ApiVersion::One {
            &[]
        } else {
            match direction {
                Direction::Pack => &[0x0d02, 0x0d03, 0x0d04],
                Direction::Unpack => &[0x0cf2, 0x0cf3, 0x0cf4, 0x806d, 0x806e],
            }
        };
        let mut saved = Vec::with_capacity(extra.len() + 1);
        for pname in std::iter::once(alignment).chain(extra.iter().copied()) {
            let mut value = 0;
            unsafe {
                gl::GetIntegerv(pname, &mut value);
                gl::PixelStorei(pname, if pname == alignment { 1 } else { 0 });
            }
            saved.push((pname, value));
        }
        Self(saved)
    }
}
impl Drop for PixelStoreGuard {
    fn drop(&mut self) {
        for &(pname, value) in &self.0 {
            unsafe { gl::PixelStorei(pname, value) };
        }
    }
}
