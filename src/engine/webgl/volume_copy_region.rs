//! CopyTexSubImage clips reads, leaving destination texels outside the source
//! framebuffer unchanged (unlike a zero-initialized readPixels destination).
use super::{Kind, Result, WebGl, gl};

pub(super) fn clip(
    source: [i32; 4],
    offsets: [i32; 3],
    extent: [u32; 2],
) -> Option<([i32; 4], [i32; 3])> {
    let [x, y, width, height] = source.map(i64::from);
    let left = x.max(0);
    let bottom = y.max(0);
    let right = (x + width).min(i64::from(extent[0]));
    let top = (y + height).min(i64::from(extent[1]));
    if left >= right || bottom >= top {
        return None;
    }
    Some((
        [
            left as i32,
            bottom as i32,
            (right - left) as i32,
            (top - bottom) as i32,
        ],
        [
            offsets[0] + (left - x) as i32,
            offsets[1] + (bottom - y) as i32,
            offsets[2],
        ],
    ))
}

impl WebGl {
    pub(super) fn volume_copy_read_extent(&mut self) -> Result<[u32; 2]> {
        if self.read_framebuffer == 0 {
            return Ok([self.surface.width, self.surface.height]);
        }
        let framebuffer = self.objects.get(self.read_framebuffer, Kind::Framebuffer)?;
        let attachment = framebuffer
            .framebuffer_attachments
            .get(&framebuffer.read_buffer)
            .ok_or(gl::INVALID_OPERATION)?;
        let object = self.objects.get(attachment.id, attachment.kind)?;
        if attachment.kind == Kind::Texture {
            let image = object
                .core_images
                .get(&(attachment.target, attachment.level))
                .ok_or(gl::INVALID_OPERATION)?;
            return Ok([image.width, image.height]);
        }
        let mut previous = 0;
        let mut width = 0;
        let mut height = 0;
        // Query the selected attachment, not the author's unrelated binding.
        // Restore before checking errors so a failed provider query is neutral.
        unsafe {
            gl::GetIntegerv(gl::RENDERBUFFER_BINDING, &mut previous);
            gl::BindRenderbuffer(gl::RENDERBUFFER, attachment.native);
            gl::GetRenderbufferParameteriv(gl::RENDERBUFFER, gl::RENDERBUFFER_WIDTH, &mut width);
            gl::GetRenderbufferParameteriv(gl::RENDERBUFFER, gl::RENDERBUFFER_HEIGHT, &mut height);
            gl::BindRenderbuffer(gl::RENDERBUFFER, previous as u32);
        }
        self.driver_result()?;
        Ok([width as u32, height as u32])
    }
}

#[cfg(test)]
mod tests {
    use super::clip;

    #[test]
    fn clipped_copy_moves_only_the_destination_origin_and_intersecting_extent() {
        assert_eq!(
            clip([-2, -1, 4, 4], [1, 2, 3], [4, 4]),
            Some(([0, 0, 2, 3], [3, 3, 3]))
        );
        assert_eq!(
            clip([2, 3, 4, 4], [0, 0, 1], [4, 4]),
            Some(([2, 3, 2, 1], [0, 0, 1]))
        );
        for source in [
            [-10, 0, 4, 4],
            [4, 0, 4, 4],
            [0, 4, 4, 4],
            [0, 0, 0, 4],
            [0, 0, 4, 0],
        ] {
            assert_eq!(clip(source, [0, 0, 0], [4, 4]), None);
        }
        assert_eq!(clip([i32::MIN, 0, i32::MAX, 4], [0, 0, 0], [4, 4]), None);
    }
}
