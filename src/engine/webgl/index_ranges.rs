//! One checked index decoder shared by ordinary and instanced indexed draws.
use super::{Kind, Result, WebGl, gl};
impl WebGl {
    pub(super) fn index_size(&self, kind: u32) -> Result<usize> {
        match kind {
            gl::UNSIGNED_BYTE => Ok(1),
            gl::UNSIGNED_SHORT => Ok(2),
            gl::UNSIGNED_INT if self.extensions.uint_indices => Ok(4),
            _ => Err(gl::INVALID_ENUM),
        }
    }
    pub(super) fn maximum_index(
        &self,
        count: usize,
        size: usize,
        offset: usize,
    ) -> Result<Option<u32>> {
        let buffer = self.objects.get(self.element_buffer, Kind::Buffer)?;
        let end = offset
            .checked_add(count.checked_mul(size).ok_or(gl::INVALID_OPERATION)?)
            .ok_or(gl::INVALID_OPERATION)?;
        let indices = buffer.bytes.get(offset..end).ok_or(gl::INVALID_OPERATION)?;
        let maximum = maximum(indices, size, self.options.api)?;
        if let Some(maximum) = maximum
            && self
                .core
                .as_ref()
                .is_some_and(|core| maximum > core.max_element_index)
        {
            return Err(gl::INVALID_OPERATION);
        }
        Ok(maximum)
    }
}

fn maximum(indices: &[u8], size: usize, api: super::ApiVersion) -> Result<Option<u32>> {
    let restart = api == super::ApiVersion::Two;
    let maximum = match size {
        1 => indices
            .iter()
            .copied()
            .map(u32::from)
            .filter(|value| !restart || *value != u8::MAX as u32)
            .max(),
        2 => indices
            .chunks_exact(2)
            .map(|v| u32::from(u16::from_ne_bytes([v[0], v[1]])))
            .filter(|value| !restart || *value != u16::MAX as u32)
            .max(),
        4 => indices
            .chunks_exact(4)
            .map(|v| u32::from_ne_bytes([v[0], v[1], v[2], v[3]]))
            .filter(|value| !restart || *value != u32::MAX)
            .max(),
        _ => return Err(gl::INVALID_ENUM),
    };
    if maximum.is_none() && !restart {
        return Err(gl::INVALID_OPERATION);
    }
    Ok(maximum)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_webgl2_excludes_fixed_restart_markers_from_attribute_bounds() {
        for size in [1, 2, 4] {
            let mut bytes = vec![0xff; size];
            bytes.extend(&42u32.to_ne_bytes()[..size]);
            let marker = match size {
                1 => 255,
                2 => 65535,
                _ => u32::MAX,
            };
            assert_eq!(
                maximum(&bytes, size, super::super::ApiVersion::One),
                Ok(Some(marker))
            );
            assert_eq!(
                maximum(&bytes, size, super::super::ApiVersion::Two),
                Ok(Some(42))
            );
            assert_eq!(
                maximum(&bytes[..size], size, super::super::ApiVersion::Two),
                Ok(None)
            );
        }
        assert_eq!(
            maximum(&[0xff], 3, super::super::ApiVersion::Two),
            Err(gl::INVALID_ENUM)
        );
    }
}
