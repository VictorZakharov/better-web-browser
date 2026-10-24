//! EXT_texture_norm16 admission stays per-context and separate from native caps.
//! The existing ANGLE provider owns filtering, normalization and renderability.
use super::texture_capabilities::TextureCapability;
use super::{ApiVersion, Command, Kind, Result, WebGl, gl};

pub(super) const FORMATS: [u32; 8] = [
    0x822a, 0x822c, 0x8054, 0x805b, 0x8f98, 0x8f99, 0x8f9a, 0x8f9b,
];
pub(super) const RENDERABLE: [u32; 3] = [0x822a, 0x822c, 0x805b];

impl WebGl {
    pub(super) fn validate_normalized_unpack_workspace(
        &self,
        bytes: Option<&[u8]>,
        retained: usize,
    ) -> Result<()> {
        if bytes.is_some_and(|bytes| {
            retained
                .checked_add(bytes.len())
                .is_none_or(|total| total > self.resource_limit)
        }) {
            return Err(gl::OUT_OF_MEMORY);
        }
        Ok(())
    }

    pub(super) fn normalized_unpack_bytes(
        &mut self,
        command: &Command,
        internal: u32,
        from_buffer: bool,
        offset_index: usize,
        size: usize,
    ) -> Result<Option<Vec<u8>>> {
        if !from_buffer || !FORMATS.contains(&internal) {
            return Ok(None);
        }
        let offset = command.i[offset_index];
        if offset % 4 == 0 {
            return Ok(None);
        }
        // The pinned D3D11 normalized-16 PBO conversion rounds a two-byte
        // source offset down to a four-byte boundary. WebGL permits every
        // offset aligned to the pixel type. Read native truth through the
        // existing bounded buffer path; never round offsets or use a stale
        // CPU mirror after GPU writes. Call only after unpack_pointer validates
        // the offset, footprint and active transform-feedback restrictions.
        if self
            .resource_bytes
            .checked_add(size)
            .is_none_or(|total| total > self.resource_limit)
        {
            return Err(gl::OUT_OF_MEMORY);
        }
        self.read_buffer(&Command {
            op: "getBufferSubData".into(),
            i: vec![
                super::core_buffers::PIXEL_UNPACK as i64,
                offset,
                size as i64,
            ],
            f: vec![],
            text: String::new(),
        })
        .map(Some)
    }

    pub(super) fn validate_normalized_texture(&self, internal: u32, error: u32) -> Result<()> {
        if FORMATS.contains(&internal)
            && (self.options.api != ApiVersion::Two
                || !self.extensions.textures.enabled(TextureCapability::Norm16))
        {
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn normalized_renderbuffer_bytes(&self, internal: u32) -> Result<Option<usize>> {
        if !FORMATS.contains(&internal) {
            return Ok(None);
        }
        self.validate_normalized_texture(internal, gl::INVALID_ENUM)?;
        // RGB16 and every signed-normalized format are filterable, not color
        // renderable. EXT_render_snorm is a distinct capability, not implied.
        // https://registry.khronos.org/webgl/extensions/EXT_texture_norm16/
        if !RENDERABLE.contains(&internal) {
            return Err(gl::INVALID_ENUM);
        }
        Ok(Some(8))
    }

    pub(super) fn normalized_read_pair(&self, format: u32, kind: u32) -> Result<bool> {
        if format != gl::RGBA || kind != gl::UNSIGNED_SHORT || self.read_framebuffer == 0 {
            return Ok(false);
        }
        if !self.extensions.textures.enabled(TextureCapability::Norm16) {
            return Ok(false);
        }
        let framebuffer = self.objects.get(self.read_framebuffer, Kind::Framebuffer)?;
        let Some(attachment) = framebuffer
            .framebuffer_attachments
            .get(&framebuffer.read_buffer)
        else {
            return Ok(false);
        };
        let object = self.objects.get(attachment.id, attachment.kind)?;
        let internal = if attachment.kind == Kind::Texture {
            object
                .core_images
                .get(&(attachment.target, attachment.level))
                .map_or(0, |image| image.internal)
        } else {
            object.renderbuffer_format
        };
        // GLES EXT_texture_norm16 §16.1.2 adds this mandatory read pair in
        // addition to RGBA/UNSIGNED_BYTE, even if the implementation pair differs.
        // https://registry.khronos.org/OpenGL/extensions/EXT/EXT_texture_norm16.txt
        Ok(RENDERABLE.contains(&internal))
    }
}
