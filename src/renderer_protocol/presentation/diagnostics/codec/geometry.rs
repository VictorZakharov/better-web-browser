use super::*;

pub(super) fn encode_rects(writer: &mut WireWriter, values: &[RectF]) -> Result<(), ProtocolError> {
    count(values.len(), MAX_RESOURCE_RECTS)?;
    writer.u32(values.len() as u32);
    for value in values {
        encode_rect(writer, *value)?;
    }
    Ok(())
}

pub(super) fn decode_rects(reader: &mut WireReader<'_>) -> Result<Vec<RectF>, ProtocolError> {
    let count = bounded_count(reader.u32()?, MAX_RESOURCE_RECTS)?;
    (0..count).map(|_| decode_rect(reader)).collect()
}

pub(super) fn encode_optional_rect(
    writer: &mut WireWriter,
    value: Option<RectF>,
) -> Result<(), ProtocolError> {
    writer.bool(value.is_some());
    if let Some(value) = value {
        encode_rect(writer, value)?;
    }
    Ok(())
}

pub(super) fn decode_optional_rect(
    reader: &mut WireReader<'_>,
) -> Result<Option<RectF>, ProtocolError> {
    reader.bool()?.then(|| decode_rect(reader)).transpose()
}

fn encode_rect(writer: &mut WireWriter, rect: RectF) -> Result<(), ProtocolError> {
    validate_rect(rect)?;
    writer.f32(rect.x);
    writer.f32(rect.y);
    writer.f32(rect.width);
    writer.f32(rect.height);
    Ok(())
}

fn decode_rect(reader: &mut WireReader<'_>) -> Result<RectF, ProtocolError> {
    let rect = RectF {
        x: reader.f32()?,
        y: reader.f32()?,
        width: reader.f32()?,
        height: reader.f32()?,
    };
    validate_rect(rect)?;
    Ok(rect)
}

fn validate_rect(rect: RectF) -> Result<(), ProtocolError> {
    if !rect.x.is_finite()
        || !rect.y.is_finite()
        || rect.x.abs() > MAX_COORDINATE
        || rect.y.abs() > MAX_COORDINATE
        || !rect.width.is_finite()
        || !rect.height.is_finite()
        || !(0.0..=MAX_COORDINATE).contains(&rect.width)
        || !(0.0..=MAX_COORDINATE).contains(&rect.height)
    {
        return invalid();
    }
    Ok(())
}
