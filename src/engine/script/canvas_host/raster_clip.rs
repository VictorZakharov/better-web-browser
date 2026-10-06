//! Map a packed whole-bitmap clip into a bounded native raster region. A clip
//! constrains final painting, not geometry generation or the source for a shadow.
use super::*;

pub(super) struct Clip<'a> {
    bits: &'a [u8],
    bitmap_width: usize,
    left: usize,
    top: usize,
    region_width: usize,
}

impl<'a> Clip<'a> {
    pub(super) fn from_args(
        args: &'a [JsValue],
        first: usize,
        [region_width, region_height]: [u32; 2],
    ) -> Option<Option<Self>> {
        let Some(value) = args.get(first) else {
            return Some(None);
        };
        if matches!(value, JsValue::Null) {
            return Some(None);
        }
        let bits = value.as_bytes()?;
        let width = super::source_layer::dimension(args.get(first + 1)?)?;
        let height = super::source_layer::dimension(args.get(first + 2)?)?;
        let left = coordinate(args.get(first + 3)?)?;
        let top = coordinate(args.get(first + 4)?)?;
        let pixels = (width as usize).checked_mul(height as usize)?;
        if width == 0
            || height == 0
            || pixels > MAX_CANVAS_PIXELS
            || bits.len() != pixels.div_ceil(8)
            || region_width == 0
            || region_height == 0
            || left.checked_add(region_width)? > width
            || top.checked_add(region_height)? > height
        {
            return None;
        }
        Some(Some(Self {
            bits,
            bitmap_width: width as usize,
            left: left as usize,
            top: top as usize,
            region_width: region_width as usize,
        }))
    }

    pub(super) fn allows(&self, index: usize) -> bool {
        let x = index % self.region_width + self.left;
        let y = index / self.region_width + self.top;
        let bit = y * self.bitmap_width + x;
        self.bits[bit / 8] & (1 << (bit % 8)) != 0
    }
}

fn coordinate(value: &JsValue) -> Option<u32> {
    let value = value.as_number()?;
    (value.is_finite() && (0.0..=16384.0).contains(&value) && value.fract() == 0.0)
        .then_some(value as u32)
}

#[cfg(test)]
mod tests;
