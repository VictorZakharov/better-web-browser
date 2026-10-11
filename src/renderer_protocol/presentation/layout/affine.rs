//! Exact, bounded retained 2D coefficients. Singular matrices are valid and paint nothing.
use super::*;
use crate::engine::affine::Affine2D;

pub(in crate::renderer_protocol::presentation) fn valid(matrix: Affine2D) -> bool {
    matrix
        .0
        .iter()
        .all(|v| v.is_finite() && v.abs() <= f64::from(MAX_PRESENTATION_COORDINATE))
}

pub(super) fn encode(writer: &mut WireWriter, matrix: Affine2D) -> Result<(), ProtocolError> {
    if !valid(matrix) {
        return Err(ProtocolError::InvalidPayload("affine matrix"));
    }
    for value in matrix.0 {
        writer.u64(value.to_bits());
    }
    Ok(())
}

pub(super) fn decode(reader: &mut WireReader<'_>) -> Result<Affine2D, ProtocolError> {
    let mut values = [0.0; 6];
    for value in &mut values {
        *value = f64::from_bits(reader.u64()?);
    }
    let matrix = Affine2D(values);
    valid(matrix)
        .then_some(matrix)
        .ok_or(ProtocolError::InvalidPayload("affine matrix"))
}

#[cfg(test)]
mod tests;
