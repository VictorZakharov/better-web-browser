use super::*;

pub(super) fn encode_rect(writer: &mut WireWriter, rect: RectF) {
    writer.f32(rect.x);
    writer.f32(rect.y);
    writer.f32(rect.width);
    writer.f32(rect.height);
}

pub(super) fn decode_rect(reader: &mut WireReader<'_>) -> Result<RectF, ProtocolError> {
    Ok(RectF {
        x: finite(
            reader.f32()?,
            -MAX_PRESENTATION_COORDINATE,
            MAX_PRESENTATION_COORDINATE,
            "rect x",
        )?,
        y: finite(
            reader.f32()?,
            -MAX_PRESENTATION_COORDINATE,
            MAX_PRESENTATION_COORDINATE,
            "rect y",
        )?,
        width: finite(
            reader.f32()?,
            0.0,
            MAX_PRESENTATION_COORDINATE,
            "rect width",
        )?,
        height: finite(
            reader.f32()?,
            0.0,
            MAX_PRESENTATION_COORDINATE,
            "rect height",
        )?,
    })
}

pub(super) fn encode_font(writer: &mut WireWriter, font: &FontSpec) -> Result<(), ProtocolError> {
    writer.string(&font.family)?;
    writer.f32(font.size);
    writer.u16(font.weight);
    writer.bool(font.italic);
    writer.bool(font.underline);
    writer.f32(font.letter_spacing);
    writer.f32(font.word_spacing);
    writer.bool(font.rtl);
    writer.bool(font.kerning);
    writer.u16(font.variants.bits());
    writer.u16(font.features.settings().len() as u16);
    for (tag, value) in font.features.settings() {
        writer.u32(u32::from_be_bytes(*tag));
        writer.u32(*value);
    }
    Ok(())
}

pub(super) fn decode_font(reader: &mut WireReader<'_>) -> Result<FontSpec, ProtocolError> {
    let family = reader.string(MAX_FAMILY_BYTES)?;
    // CSS font sizes are non-negative and may legitimately be zero or subpixel-sized. Text
    // rasterization applies its own implementation floor, but the checked wire contract must not
    // reject valid computed values before the presentation reaches that boundary.
    let size = finite(reader.f32()?, 0.0, 768.0, "font size")?;
    let weight = reader.u16()?;
    if !(1..=1000).contains(&weight) {
        return Err(ProtocolError::InvalidPayload("font weight"));
    }
    let italic = reader.bool()?;
    let underline = reader.bool()?;
    let letter_spacing = finite(reader.f32()?, -768.0, 768.0, "letter spacing")?;
    let word_spacing = finite(reader.f32()?, -768.0, 768.0, "word spacing")?;
    let rtl = reader.bool()?;
    let kerning = reader.bool()?;
    let variants = crate::engine::css::FontVariants::from_bits(reader.u16()?)
        .ok_or(ProtocolError::InvalidPayload("font variant flags"))?;
    let count = reader.u16()? as usize;
    if count > 64 {
        return Err(ProtocolError::InvalidPayload("font feature count"));
    }
    let mut settings = Vec::with_capacity(count);
    for _ in 0..count {
        settings.push((reader.u32()?.to_be_bytes(), reader.u32()?));
    }
    let features = crate::engine::css::FontFeatures::from_settings(&settings)
        .ok_or(ProtocolError::InvalidPayload("font feature tag"))?;
    Ok(FontSpec {
        family,
        size,
        weight,
        italic,
        underline,
        letter_spacing,
        word_spacing,
        rtl,
        kerning,
        features,
        variants,
    })
}

#[cfg(test)]
mod text_tests;

pub(super) fn encode_edges(writer: &mut WireWriter, values: [f32; 4]) {
    for value in values {
        writer.f32(value);
    }
}

pub(super) fn decode_edges(reader: &mut WireReader<'_>) -> Result<[f32; 4], ProtocolError> {
    Ok([
        finite(reader.f32()?, 0.0, MAX_PRESENTATION_COORDINATE, "edge")?,
        finite(reader.f32()?, 0.0, MAX_PRESENTATION_COORDINATE, "edge")?,
        finite(reader.f32()?, 0.0, MAX_PRESENTATION_COORDINATE, "edge")?,
        finite(reader.f32()?, 0.0, MAX_PRESENTATION_COORDINATE, "edge")?,
    ])
}

pub(super) fn encode_color(writer: &mut WireWriter, color: Color) {
    writer.u8(color.red);
    writer.u8(color.green);
    writer.u8(color.blue);
    writer.u8(color.alpha);
}

pub(super) fn decode_color(reader: &mut WireReader<'_>) -> Result<Color, ProtocolError> {
    Ok(Color {
        red: reader.u8()?,
        green: reader.u8()?,
        blue: reader.u8()?,
        alpha: reader.u8()?,
    })
}

pub(super) fn encode_optional_string(
    writer: &mut WireWriter,
    value: Option<&str>,
) -> Result<(), ProtocolError> {
    writer.bool(value.is_some());
    if let Some(value) = value {
        writer.string(value)?;
    }
    Ok(())
}

pub(super) fn decode_optional_string(
    reader: &mut WireReader<'_>,
    maximum: usize,
) -> Result<Option<String>, ProtocolError> {
    reader.bool()?.then(|| reader.string(maximum)).transpose()
}

pub(super) fn decode_node_id(reader: &mut WireReader<'_>) -> Result<NodeId, ProtocolError> {
    NodeId::from_wire(reader.u128()?).ok_or(ProtocolError::InvalidPayload("node identifier"))
}

pub(super) fn finite(
    value: f32,
    minimum: f32,
    maximum: f32,
    field: &'static str,
) -> Result<f32, ProtocolError> {
    if value.is_finite() && (minimum..=maximum).contains(&value) {
        Ok(value)
    } else {
        Err(ProtocolError::InvalidPayload(field))
    }
}

pub(super) fn bounded_count(
    value: u32,
    maximum: usize,
    field: &'static str,
) -> Result<usize, ProtocolError> {
    let value = value as usize;
    (value <= maximum)
        .then_some(value)
        .ok_or(ProtocolError::InvalidPayload(field))
}
