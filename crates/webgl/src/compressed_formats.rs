//! Closed BC1/2/3/4/5 footprint and block-update rules from Khronos extensions.
use super::compressed_capabilities::Family;
use super::{Result, gl};
#[derive(Clone, Copy)]
pub(super) struct Format {
    pub family: Family,
    pub block_bytes: usize,
}
pub(super) fn format(internal: u32) -> Result<Format> {
    let family = Family::ALL
        .into_iter()
        .find(|family| family.formats().contains(&internal))
        .ok_or(gl::INVALID_ENUM)?;
    let block_bytes = if [0x83f0, 0x83f1, 0x8c4c, 0x8c4d, 0x8dbb, 0x8dbc].contains(&internal) {
        8
    } else {
        16
    };
    Ok(Format {
        family,
        block_bytes,
    })
}
impl Format {
    pub(super) fn byte_size(self, width: u32, height: u32) -> Result<usize> {
        (width as usize)
            .div_ceil(4)
            .checked_mul((height as usize).div_ceil(4))
            .and_then(|blocks| blocks.checked_mul(self.block_bytes))
            .ok_or(gl::OUT_OF_MEMORY)
    }
    pub(super) fn image_dimensions(self, width: u32, height: u32, level: u32) -> Result<()> {
        let legal = |dimension: u32| match self.family {
            Family::Srgb => dimension.is_multiple_of(4) || (level > 0 && dimension <= 2),
            // Widen before shifting: a valid tiny mip can reconstruct a large
            // virtual base dimension without overflowing a native GLsizei.
            _ => level <= 31 && ((dimension as u64) << level).is_multiple_of(4),
        };
        if legal(width) && legal(height) {
            Ok(())
        } else {
            Err(gl::INVALID_OPERATION)
        }
    }
    pub(super) fn subregion(
        self,
        image: (u32, u32),
        offset: (u32, u32),
        size: (u32, u32),
    ) -> Result<()> {
        let (x, y) = offset;
        let (width, height) = size;
        if x.checked_add(width).is_none_or(|end| end > image.0)
            || y.checked_add(height).is_none_or(|end| end > image.1)
        {
            return Err(gl::INVALID_VALUE);
        }
        let valid_width = width.is_multiple_of(4)
            || match self.family {
                // RGTC supports partial edge tiles, not only complete-level updates.
                Family::Rgtc => x + width == image.0,
                _ => width == image.0,
            };
        let valid_height = height.is_multiple_of(4)
            || match self.family {
                Family::Rgtc => y + height == image.1,
                _ => height == image.1,
            };
        if !x.is_multiple_of(4) || !y.is_multiple_of(4) || !valid_width || !valid_height {
            return Err(gl::INVALID_OPERATION);
        }
        Ok(())
    }
}
