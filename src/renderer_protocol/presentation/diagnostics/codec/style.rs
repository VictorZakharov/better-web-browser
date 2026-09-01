use super::*;

const MAX_CUSTOM_PROPERTIES: usize = 64;

pub(super) fn encode_style(
    writer: &mut WireWriter,
    value: &StyleDiagnostics,
) -> Result<(), ProtocolError> {
    for text in [
        &value.display,
        &value.position,
        &value.float,
        &value.flex_direction,
        &value.flex_basis,
        &value.align_items,
        &value.justify_content,
        &value.list_style_type,
        &value.width,
        &value.height,
        &value.min_width,
        &value.max_width,
        &value.min_height,
        &value.max_height,
        &value.background_color,
    ] {
        string(writer, text, MAX_DIAGNOSTIC_TEXT_BYTES)?;
    }
    writer.bool(value.visibility);
    if !value.opacity.is_finite() || !(0.0..=1.0).contains(&value.opacity) {
        return invalid();
    }
    writer.f32(value.opacity);
    writer.bool(value.overflow_hidden);
    writer.bool(value.flex_wrap);
    for number in [value.flex_grow, value.flex_shrink] {
        if !number.is_finite() || number < 0.0 {
            return invalid();
        }
        writer.f32(number);
    }
    encode_optional_resource(writer, value.background_image.as_ref())?;
    encode_optional_resource(writer, value.mask_image.as_ref())?;
    writer.u64(value.custom_property_count);
    writer.bool(value.custom_properties_truncated);
    count(value.custom_properties.len(), MAX_CUSTOM_PROPERTIES)?;
    writer.u32(value.custom_properties.len() as u32);
    for property in &value.custom_properties {
        string(writer, &property.name, MAX_DIAGNOSTIC_TEXT_BYTES)?;
        string(writer, &property.value, MAX_DIAGNOSTIC_TEXT_BYTES)?;
    }
    if value.custom_property_count < value.custom_properties.len() as u64
        || value.custom_properties_truncated
            != (value.custom_property_count > value.custom_properties.len() as u64)
    {
        return invalid();
    }
    Ok(())
}

pub(super) fn decode_style(reader: &mut WireReader<'_>) -> Result<StyleDiagnostics, ProtocolError> {
    let display = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let position = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let float = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let flex_direction = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let flex_basis = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let align_items = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let justify_content = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let list_style_type = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let width = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let height = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let min_width = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let max_width = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let min_height = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let max_height = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let background_color = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let visibility = reader.bool()?;
    let opacity = reader.f32()?;
    if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
        return invalid();
    }
    let overflow_hidden = reader.bool()?;
    let flex_wrap = reader.bool()?;
    let flex_grow = reader.f32()?;
    let flex_shrink = reader.f32()?;
    if !flex_grow.is_finite() || flex_grow < 0.0 || !flex_shrink.is_finite() || flex_shrink < 0.0 {
        return invalid();
    }
    let background_image = decode_optional_resource(reader)?;
    let mask_image = decode_optional_resource(reader)?;
    let custom_property_count = reader.u64()?;
    let custom_properties_truncated = reader.bool()?;
    let property_count = bounded_count(reader.u32()?, MAX_CUSTOM_PROPERTIES)?;
    let mut custom_properties = Vec::with_capacity(property_count);
    for _ in 0..property_count {
        custom_properties.push(CustomPropertyDiagnostics {
            name: reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?,
            value: reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?,
        });
    }
    if custom_property_count < property_count as u64
        || custom_properties_truncated != (custom_property_count > property_count as u64)
    {
        return invalid();
    }
    Ok(StyleDiagnostics {
        display,
        position,
        float,
        flex_direction,
        flex_wrap,
        flex_grow,
        flex_shrink,
        flex_basis,
        align_items,
        justify_content,
        visibility,
        opacity,
        overflow_hidden,
        list_style_type,
        width,
        height,
        min_width,
        max_width,
        min_height,
        max_height,
        background_color,
        background_image,
        mask_image,
        custom_property_count,
        custom_properties_truncated,
        custom_properties,
    })
}
