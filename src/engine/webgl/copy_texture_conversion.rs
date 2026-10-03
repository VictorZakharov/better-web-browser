//! WebGL1's floating color-buffer extensions permit normalized full/subcopies.
//! Apply the same closed conversion across native providers. Read through
//! bounded, owned RGBA floats, then convert only the destination's channels.
use super::{MAX_UPLOAD_BYTES, Result, WebGl, gl};

impl WebGl {
    pub(super) fn copy_float_to_normalized(
        &mut self,
        target: u32,
        level: i32,
        format: u32,
        rectangle: [i32; 4],
        offset: Option<[i32; 2]>,
    ) -> Result<()> {
        let [x, y, width, height] = rectangle;
        let count = (width as usize)
            .checked_mul(height as usize)
            .ok_or(gl::OUT_OF_MEMORY)?;
        if count > MAX_UPLOAD_BYTES / 16 {
            return Err(gl::OUT_OF_MEMORY);
        }
        let mut alpha_bits = 0;
        let mut pack = 0;
        let mut unpack = 0;
        unsafe {
            gl::GetIntegerv(gl::ALPHA_BITS, &mut alpha_bits);
            gl::GetIntegerv(gl::PACK_ALIGNMENT, &mut pack);
            gl::GetIntegerv(gl::UNPACK_ALIGNMENT, &mut unpack);
        }
        self.driver_result()?;
        if alpha_bits == 0 && [gl::ALPHA, gl::LUMINANCE_ALPHA, gl::RGBA].contains(&format) {
            return Err(gl::INVALID_OPERATION);
        }
        let mut pixels = vec![0.0f32; count * 4];
        unsafe {
            gl::PixelStorei(gl::PACK_ALIGNMENT, 1);
            gl::ReadPixels(
                x,
                y,
                width,
                height,
                gl::RGBA,
                gl::FLOAT,
                pixels.as_mut_ptr().cast(),
            );
        }
        let read = self.driver_result();
        unsafe {
            gl::PixelStorei(gl::PACK_ALIGNMENT, pack);
        }
        let restored = self.driver_result();
        read?;
        restored?;
        let bytes = normalized_channels(&pixels, format);
        unsafe {
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
            if let Some([xoffset, yoffset]) = offset {
                gl::TexSubImage2D(
                    target,
                    level,
                    xoffset,
                    yoffset,
                    width,
                    height,
                    format,
                    gl::UNSIGNED_BYTE,
                    bytes.as_ptr().cast(),
                );
            } else {
                gl::TexImage2D(
                    target,
                    level,
                    format as i32,
                    width,
                    height,
                    0,
                    format,
                    gl::UNSIGNED_BYTE,
                    bytes.as_ptr().cast(),
                );
            }
        }
        let upload = self.driver_result();
        unsafe {
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, unpack);
        }
        let restored = self.driver_result();
        upload?;
        restored
    }
}

fn normalized_channels(pixels: &[f32], format: u32) -> Vec<u8> {
    let channels: &[usize] = match format {
        gl::ALPHA => &[3],
        gl::LUMINANCE => &[0],
        gl::LUMINANCE_ALPHA => &[0, 3],
        gl::RGB => &[0, 1, 2],
        _ => &[0, 1, 2, 3],
    };
    let mut bytes = Vec::with_capacity(pixels.len() / 4 * channels.len());
    for pixel in pixels.chunks_exact(4) {
        for &channel in channels {
            // Float framebuffer values may be HDR, negative, or non-finite.
            // Rust's saturating cast maps NaN to zero without undefined behavior.
            bytes.push((pixel[channel].clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_copy_selects_channels_clamps_hdr_and_handles_nonfinite_values() {
        let pixels = [2.0, -1.0, 0.5, 0.25];
        for (format, expected) in [
            (gl::RGBA, vec![255, 0, 128, 64]),
            (gl::RGB, vec![255, 0, 128]),
            (gl::ALPHA, vec![64]),
            (gl::LUMINANCE, vec![255]),
            (gl::LUMINANCE_ALPHA, vec![255, 64]),
        ] {
            assert_eq!(normalized_channels(&pixels, format), expected);
        }
        assert_eq!(
            normalized_channels(&[f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.0], gl::RGBA),
            [0, 255, 0, 255]
        );
        assert!(normalized_channels(&[], gl::RGBA).is_empty());
    }
}
