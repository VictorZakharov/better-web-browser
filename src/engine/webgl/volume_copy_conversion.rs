//! A bounded actual-pixel fallback for formats whose ANGLE 3D copy paths
//! corrupt neighboring layers. Native readback and regional upload retain
//! source values and existing GPU-written destination storage.
use super::core_texture_formats as formats;
use super::pixel_buffer_guard::{Direction as BufferDirection, PixelBufferGuard};
use super::pixel_layout::Direction as StoreDirection;
use super::pixel_store_guard::PixelStoreGuard;
use super::{Kind, MAX_UPLOAD_BYTES, Result, WebGl, gl};

impl WebGl {
    pub(super) fn copy_volume_through_native_conversion(
        &mut self,
        id: u32,
        level: i32,
        offsets: [i32; 3],
        source: [i32; 4],
    ) -> Result<()> {
        let image = *self
            .objects
            .get(id, Kind::Texture)?
            .core_images
            .get(&(super::texture_targets::VOLUME, level))
            .ok_or(gl::INVALID_OPERATION)?;
        let plan = Transfer::for_format(image.internal)?;
        let Some((source, offsets)) =
            super::volume_copy_region::clip(source, offsets, self.volume_copy_read_extent()?)
        else {
            return Ok(());
        };
        let [x, y, width, height] = source;
        let pixels = (width as usize)
            .checked_mul(height as usize)
            .ok_or(gl::OUT_OF_MEMORY)?;
        // Charge both simultaneous owned buffers, not just the smaller RGB
        // upload. No context-global mirror is consulted or replaced.
        let read_size = pixels.checked_mul(16).ok_or(gl::OUT_OF_MEMORY)?;
        let upload_size = pixels
            .checked_mul(3 * plan.bytes())
            .ok_or(gl::OUT_OF_MEMORY)?;
        let capacity = read_size
            .checked_add(upload_size)
            .ok_or(gl::OUT_OF_MEMORY)?;
        if capacity > MAX_UPLOAD_BYTES
            || self
                .resource_bytes
                .checked_add(capacity)
                .is_none_or(|total| total > self.resource_limit)
        {
            return Err(gl::OUT_OF_MEMORY);
        }
        let _pack = PixelStoreGuard::tight(self.options.api, StoreDirection::Pack);
        let _unpack = PixelStoreGuard::tight(self.options.api, StoreDirection::Unpack);
        let _pack_buffer = PixelBufferGuard::unbind(self.options.api, BufferDirection::Pack);
        let _unpack_buffer = PixelBufferGuard::unbind(self.options.api, BufferDirection::Unpack);
        // This is a private copy, not the author client-memory overload. The
        // native guards unbind PBOs while browser binding metadata stays intact.
        // Calling the public read path here would reject a saved PACK buffer.
        self.core_read_pair(plan.read_format(), plan.read_type())?;
        let mut source = vec![0u8; read_size];
        unsafe {
            gl::ReadPixels(
                x,
                y,
                width,
                height,
                plan.read_format(),
                plan.read_type(),
                source.as_mut_ptr().cast(),
            );
        }
        self.driver_result()?;
        let bytes = plan.select_rgb(&source)?;
        let upload = self
            .core
            .as_ref()
            .ok_or(gl::INVALID_OPERATION)?
            .texture_sub_image_3d;
        let [dx, dy, dz] = offsets;
        // Bounds, source/destination format compatibility, feedback, and MSAA
        // were validated by CopyTexSubImage3D before this fallback was selected.
        // TexSubImage3D updates exactly this region, not the entire volume.
        unsafe {
            upload(
                super::texture_targets::VOLUME,
                level,
                dx,
                dy,
                dz,
                width,
                height,
                1,
                plan.upload_format(),
                plan.upload_type(),
                bytes.as_ptr().cast(),
            );
        }
        self.driver_result()
    }
}

#[derive(Clone, Copy)]
pub(super) enum Transfer {
    Float,
    Signed(u8),
    Unsigned(u8),
}
impl Transfer {
    pub(super) fn for_format(internal: u32) -> Result<Self> {
        Ok(match internal {
            0x8815 | 0x881b => Self::Float,
            0x8d8f => Self::Signed(1),
            0x8d89 => Self::Signed(2),
            0x8d83 => Self::Signed(4),
            0x8d7d => Self::Unsigned(1),
            0x8d77 => Self::Unsigned(2),
            0x8d71 => Self::Unsigned(4),
            _ => return Err(gl::INVALID_OPERATION),
        })
    }
    fn bytes(self) -> usize {
        match self {
            Self::Float => 4,
            Self::Signed(bytes) | Self::Unsigned(bytes) => bytes as usize,
        }
    }
    fn read_format(self) -> u32 {
        match self {
            Self::Float => gl::RGBA,
            _ => formats::RGBA_INTEGER,
        }
    }
    fn read_type(self) -> u32 {
        match self {
            Self::Float => gl::FLOAT,
            Self::Signed(_) => gl::INT,
            Self::Unsigned(_) => gl::UNSIGNED_INT,
        }
    }
    fn upload_format(self) -> u32 {
        match self {
            Self::Float => gl::RGB,
            _ => formats::RGB_INTEGER,
        }
    }
    fn upload_type(self) -> u32 {
        match self {
            Self::Float => gl::FLOAT,
            Self::Signed(1) => gl::BYTE,
            Self::Signed(2) => gl::SHORT,
            Self::Unsigned(1) => gl::UNSIGNED_BYTE,
            Self::Unsigned(2) => gl::UNSIGNED_SHORT,
            Self::Signed(_) => gl::INT,
            Self::Unsigned(_) => gl::UNSIGNED_INT,
        }
    }
    pub(super) fn select_rgb(self, source: &[u8]) -> Result<Vec<u8>> {
        if !source.len().is_multiple_of(16) {
            return Err(gl::INVALID_OPERATION);
        }
        let mut output = Vec::with_capacity(source.len() / 16 * 3 * self.bytes());
        for pixel in source.chunks_exact(16) {
            for component in pixel[..12].chunks_exact(4) {
                let bits: [u8; 4] = component.try_into().unwrap();
                match self {
                    Self::Float | Self::Signed(4) | Self::Unsigned(4) => {
                        output.extend_from_slice(&bits)
                    }
                    Self::Signed(1) => output.push(
                        i8::try_from(i32::from_ne_bytes(bits)).map_err(|_| gl::INVALID_OPERATION)?
                            as u8,
                    ),
                    Self::Signed(2) => output.extend_from_slice(
                        &i16::try_from(i32::from_ne_bytes(bits))
                            .map_err(|_| gl::INVALID_OPERATION)?
                            .to_ne_bytes(),
                    ),
                    Self::Unsigned(1) => output.push(
                        u8::try_from(u32::from_ne_bytes(bits))
                            .map_err(|_| gl::INVALID_OPERATION)?,
                    ),
                    Self::Unsigned(2) => output.extend_from_slice(
                        &u16::try_from(u32::from_ne_bytes(bits))
                            .map_err(|_| gl::INVALID_OPERATION)?
                            .to_ne_bytes(),
                    ),
                    _ => return Err(gl::INVALID_OPERATION),
                }
            }
        }
        Ok(output)
    }
}
