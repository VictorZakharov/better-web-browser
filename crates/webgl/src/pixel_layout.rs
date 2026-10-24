//! Checked byte footprints for GLES3 pixel subrectangles and volume slices.
use super::{ApiVersion, Result, gl};
#[derive(Clone, Copy)]
pub(super) enum Direction {
    Pack,
    Unpack,
}
pub(super) fn extended_parameter(pname: u32) -> bool {
    [
        0x0d02, 0x0d03, 0x0d04, 0x0cf2, 0x0cf3, 0x0cf4, 0x806d, 0x806e,
    ]
    .contains(&pname)
}
#[derive(Clone, Copy, Default)]
pub(super) struct Store {
    pub alignment: usize,
    pub row_length: usize,
    pub skip_rows: usize,
    pub skip_pixels: usize,
    pub image_height: usize,
    pub skip_images: usize,
}
pub(super) struct Layout {
    pub size: usize,
    pub start: usize,
    pub row_stride: usize,
}
impl Store {
    pub(super) fn native(api: ApiVersion, direction: Direction) -> Result<Self> {
        let (alignment, row, rows, pixels) = match direction {
            Direction::Pack => (gl::PACK_ALIGNMENT, 0x0d02, 0x0d03, 0x0d04),
            Direction::Unpack => (gl::UNPACK_ALIGNMENT, 0x0cf2, 0x0cf3, 0x0cf4),
        };
        fn scalar(pname: u32) -> Result<usize> {
            let mut value = 0;
            unsafe { gl::GetIntegerv(pname, &mut value) };
            usize::try_from(value).map_err(|_| gl::INVALID_OPERATION)
        }
        let mut state = Self {
            alignment: scalar(alignment)?,
            ..Self::default()
        };
        if api == ApiVersion::Two {
            state.row_length = scalar(row)?;
            state.skip_rows = scalar(rows)?;
            state.skip_pixels = scalar(pixels)?;
            if matches!(direction, Direction::Unpack) {
                state.image_height = scalar(0x806e)?;
                state.skip_images = scalar(0x806d)?;
            }
        }
        Ok(state)
    }
    pub(super) fn layout(
        self,
        width: usize,
        height: usize,
        depth: usize,
        bytes: usize,
        volume: bool,
    ) -> Result<Layout> {
        if ![1, 2, 4, 8].contains(&self.alignment) || bytes == 0 || bytes > 16 {
            return Err(gl::INVALID_OPERATION);
        }
        let rows = if self.image_height == 0 || !volume {
            height
        } else {
            self.image_height
        };
        let columns = if self.row_length == 0 {
            width
        } else {
            self.row_length
        };
        if self
            .skip_pixels
            .checked_add(width)
            .is_none_or(|end| end > columns)
            || volume
                && self
                    .skip_rows
                    .checked_add(height)
                    .is_none_or(|end| end > rows)
        {
            return Err(gl::INVALID_OPERATION);
        }
        let row_bytes = columns.checked_mul(bytes).ok_or(gl::INVALID_OPERATION)?;
        let row_stride = row_bytes
            .checked_add(self.alignment - 1)
            .ok_or(gl::INVALID_OPERATION)?
            / self.alignment
            * self.alignment;
        if width == 0 || height == 0 || depth == 0 {
            return Ok(Layout {
                size: 0,
                start: 0,
                row_stride,
            });
        }
        let slice_stride = row_stride.checked_mul(rows).ok_or(gl::INVALID_OPERATION)?;
        let skip_images = if volume { self.skip_images } else { 0 };
        let start = skip_images
            .checked_mul(slice_stride)
            .and_then(|n| {
                self.skip_rows
                    .checked_mul(row_stride)
                    .and_then(|r| n.checked_add(r))
            })
            .and_then(|n| {
                self.skip_pixels
                    .checked_mul(bytes)
                    .and_then(|p| n.checked_add(p))
            })
            .ok_or(gl::INVALID_OPERATION)?;
        let size = (depth - 1)
            .checked_mul(slice_stride)
            .and_then(|n| n.checked_add(start))
            .and_then(|n| {
                (height - 1)
                    .checked_mul(row_stride)
                    .and_then(|r| n.checked_add(r))
            })
            .and_then(|n| width.checked_mul(bytes).and_then(|r| n.checked_add(r)))
            .ok_or(gl::INVALID_OPERATION)?;
        Ok(Layout {
            size,
            start,
            row_stride,
        })
    }
}
