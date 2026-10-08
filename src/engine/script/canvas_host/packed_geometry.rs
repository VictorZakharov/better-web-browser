//! Private, versioned Canvas geometry transport, not a new web API. Inputs
//! have already been copied from V8; no author/shared buffer is borrowed.
use super::path::{MAX_COORDINATE, MAX_POINTS, Part};

// Little-endian "CPG1", followed by the part count. Each part contains a u32
// point count, a u32 closed flag (0/1), then pairs of little-endian f64 values.
// Preserve the JavaScript coordinates until the existing f32 raster boundary.
pub(super) const MAGIC: u32 = u32::from_le_bytes(*b"CPG1");
pub(super) const MAX_BYTES: usize = 8 + MAX_POINTS * (8 + 16);

pub(super) fn decode(bytes: &[u8]) -> Option<Vec<Part>> {
    if bytes.len() > MAX_BYTES {
        return None;
    }
    let mut reader = Reader { bytes, offset: 0 };
    if reader.word()? != MAGIC {
        return None;
    }
    let count = reader.word()? as usize;
    if count > MAX_POINTS || count.checked_mul(8)? > reader.remaining() {
        return None;
    }
    let mut parts = Vec::with_capacity(count);
    let mut total = 0usize;
    for _ in 0..count {
        let points = reader.word()? as usize;
        total = total.checked_add(points)?;
        if total > MAX_POINTS || points.checked_mul(16)? > reader.remaining() {
            return None;
        }
        let closed = match reader.word()? {
            0 => false,
            1 => true,
            _ => return None,
        };
        // Check the exact remaining coordinate bytes before reserving storage.
        if points.checked_mul(16)? > reader.remaining() {
            return None;
        }
        let mut coordinates = Vec::with_capacity(points);
        for _ in 0..points {
            coordinates.push([reader.coordinate()?, reader.coordinate()?]);
        }
        parts.push(Part {
            points: coordinates,
            closed,
        });
    }
    (reader.remaining() == 0).then_some(parts)
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl Reader<'_> {
    fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }

    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let end = self.offset.checked_add(N)?;
        let value = self.bytes.get(self.offset..end)?.try_into().ok()?;
        self.offset = end;
        Some(value)
    }

    fn word(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take()?))
    }

    fn coordinate(&mut self) -> Option<f32> {
        let value = f64::from_le_bytes(self.take()?) as f32;
        // This matches the existing JSON provider's validation *after* f32
        // conversion, including finite doubles that round onto the boundary.
        (value.is_finite() && value.abs() <= MAX_COORDINATE).then_some(value)
    }
}

#[cfg(test)]
pub(super) fn encode(parts: &[(bool, Vec<[f64; 2]>)]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend(MAGIC.to_le_bytes());
    bytes.extend((parts.len() as u32).to_le_bytes());
    for (closed, points) in parts {
        bytes.extend((points.len() as u32).to_le_bytes());
        bytes.extend(u32::from(*closed).to_le_bytes());
        for point in points {
            bytes.extend(point[0].to_le_bytes());
            bytes.extend(point[1].to_le_bytes());
        }
    }
    bytes
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod coverage_tests;
