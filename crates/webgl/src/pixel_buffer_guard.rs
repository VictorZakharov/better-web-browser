//! Private CPU pixel operations never reinterpret their owned pointers as PBO offsets.
use super::{ApiVersion, core_buffers, gl};

pub(super) enum Direction {
    Pack,
    Unpack,
}

pub(super) struct PixelBufferGuard {
    binding: Option<(u32, u32)>,
}

impl PixelBufferGuard {
    pub(super) fn unbind(api: ApiVersion, direction: Direction) -> Self {
        if api == ApiVersion::One {
            return Self { binding: None };
        }
        let (target, pname) = match direction {
            Direction::Pack => (core_buffers::PIXEL_PACK, 0x88ed),
            Direction::Unpack => (core_buffers::PIXEL_UNPACK, 0x88ef),
        };
        let mut previous = 0;
        // SAFETY: GLES3 scalar native binding query and closed buffer target.
        unsafe {
            gl::GetIntegerv(pname, &mut previous);
            gl::BindBuffer(target, 0);
        }
        Self {
            binding: Some((target, previous as u32)),
        }
    }
}

impl Drop for PixelBufferGuard {
    fn drop(&mut self) {
        if let Some((target, previous)) = self.binding {
            // SAFETY: guard is stack-owned on the native context's owner thread;
            // the original buffer cannot be deleted while the operation runs.
            unsafe {
                gl::BindBuffer(target, previous);
            }
        }
    }
}
