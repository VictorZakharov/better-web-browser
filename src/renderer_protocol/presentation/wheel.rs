//! Ordered wheel default-action verdicts; the containing report owns the document.
use super::ProtocolError;
use crate::renderer_protocol::wire::{WireReader, WireWriter};

pub const MAX_WHEEL_ACKNOWLEDGEMENTS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelDecision {
    Cancelled,
    NestedScroll,
    Viewport,
    NoMotion,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelAcknowledgement {
    pub sequence: u64,
    pub decision: WheelDecision,
    /// Accepted viewport default-action distance in CSS pixels for this input.
    /// Compaction clears contributions superseded by a later absolute scroll;
    /// other decisions never contribute viewport motion. This is not a position.
    pub viewport_delta_y: f32,
    /// Renderer-local wheel_input span, including its dispatch/default-action work.
    /// Excludes later presentation style/layout, serialization and outbound IPC.
    pub dispatch_micros: u64,
}

pub(super) fn validate(values: &[WheelAcknowledgement]) -> Result<(), ProtocolError> {
    if values.len() > MAX_WHEEL_ACKNOWLEDGEMENTS
        || values.first().is_some_and(|value| value.sequence == 0)
        || values
            .windows(2)
            .any(|pair| pair[0].sequence >= pair[1].sequence)
        || values.iter().any(|value| {
            !value.viewport_delta_y.is_finite()
                || (value.decision != WheelDecision::Viewport && value.viewport_delta_y != 0.0)
        })
    {
        return Err(ProtocolError::InvalidPayload("wheel acknowledgements"));
    }
    Ok(())
}

pub(super) fn encode(
    writer: &mut WireWriter,
    values: &[WheelAcknowledgement],
) -> Result<(), ProtocolError> {
    validate(values)?;
    writer.u32(values.len() as u32);
    for value in values {
        writer.u64(value.sequence);
        writer.u8(match value.decision {
            WheelDecision::Cancelled => 1,
            WheelDecision::NestedScroll => 2,
            WheelDecision::Viewport => 3,
            WheelDecision::NoMotion => 4,
        });
        writer.f32(value.viewport_delta_y);
        writer.u64(value.dispatch_micros);
    }
    Ok(())
}

pub(super) fn decode(
    reader: &mut WireReader<'_>,
) -> Result<Vec<WheelAcknowledgement>, ProtocolError> {
    let count = reader.u32()? as usize;
    if count > MAX_WHEEL_ACKNOWLEDGEMENTS {
        return Err(ProtocolError::InvalidPayload("wheel acknowledgements"));
    }
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(WheelAcknowledgement {
            sequence: reader.u64()?,
            decision: match reader.u8()? {
                1 => WheelDecision::Cancelled,
                2 => WheelDecision::NestedScroll,
                3 => WheelDecision::Viewport,
                4 => WheelDecision::NoMotion,
                _ => return Err(ProtocolError::InvalidPayload("wheel decision")),
            },
            viewport_delta_y: reader.f32()?,
            dispatch_micros: reader.u64()?,
        });
    }
    validate(&values)?;
    Ok(values)
}
