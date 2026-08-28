use super::*;

pub(super) fn encode_control(
    writer: &mut WireWriter,
    spec: &ControlSpec,
) -> Result<(), ProtocolError> {
    writer.u128(spec.node_id.to_wire());
    encode_rect(writer, spec.rect);
    writer.u8(control_kind_tag(spec.kind));
    for value in [&spec.name, &spec.value, &spec.label] {
        writer.string(value)?;
    }
    writer.u32(spec.options.len() as u32);
    for option in &spec.options {
        writer.string(&option.value)?;
        writer.string(&option.label)?;
    }
    writer.u32(spec.selected_index as u32);
    writer.string(&spec.placeholder)?;
    writer.bool(spec.form_id.is_some());
    if let Some(form) = spec.form_id {
        writer.u128(form.to_wire());
    }
    encode_color(writer, spec.background_color);
    encode_color(writer, spec.text_color);
    encode_color(writer, spec.border_color);
    encode_edges(writer, spec.border_width);
    writer.f32(spec.border_radius);
    encode_edges(writer, spec.padding);
    encode_font(writer, &spec.font)?;
    encode_optional_string(writer, spec.icon_url.as_deref())?;
    writer.f32(spec.icon_width);
    writer.f32(spec.icon_height);
    Ok(())
}

pub(super) fn decode_control(reader: &mut WireReader<'_>) -> Result<ControlSpec, ProtocolError> {
    let node_id = decode_node_id(reader)?;
    let rect = decode_rect(reader)?;
    let kind = decode_control_kind(reader.u8()?)?;
    let name = reader.string(MAX_CONTROL_TEXT_BYTES)?;
    let value = reader.string(MAX_CONTROL_TEXT_BYTES)?;
    let label = reader.string(MAX_CONTROL_TEXT_BYTES)?;
    let option_count = bounded_count(reader.u32()?, MAX_CONTROL_OPTIONS, "control options")?;
    let mut options = Vec::with_capacity(option_count);
    for _ in 0..option_count {
        options.push(SelectOption {
            value: reader.string(MAX_CONTROL_TEXT_BYTES)?,
            label: reader.string(MAX_CONTROL_TEXT_BYTES)?,
        });
    }
    let selected_index = reader.u32()? as usize;
    if !options.is_empty() && selected_index >= options.len() {
        return Err(ProtocolError::InvalidPayload("selected option"));
    }
    let placeholder = reader.string(MAX_CONTROL_TEXT_BYTES)?;
    let form_id = reader.bool()?.then(|| decode_node_id(reader)).transpose()?;
    Ok(ControlSpec {
        node_id,
        rect,
        kind,
        name,
        value,
        label,
        options,
        selected_index,
        placeholder,
        form_id,
        background_color: decode_color(reader)?,
        text_color: decode_color(reader)?,
        border_color: decode_color(reader)?,
        border_width: decode_edges(reader)?,
        border_radius: finite(
            reader.f32()?,
            0.0,
            MAX_PRESENTATION_COORDINATE,
            "control radius",
        )?,
        padding: decode_edges(reader)?,
        font: decode_font(reader)?,
        icon_url: decode_optional_string(reader, MAX_URL_BYTES)?,
        icon_width: finite(
            reader.f32()?,
            0.0,
            MAX_PRESENTATION_COORDINATE,
            "control icon width",
        )?,
        icon_height: finite(
            reader.f32()?,
            0.0,
            MAX_PRESENTATION_COORDINATE,
            "control icon height",
        )?,
    })
}

pub(super) fn encode_form(writer: &mut WireWriter, form: &FormSpec) -> Result<(), ProtocolError> {
    writer.u128(form.node_id.to_wire());
    writer.string(&form.action)?;
    writer.string(&form.method)?;
    writer.u32(form.hidden_fields.len() as u32);
    for (name, value) in &form.hidden_fields {
        writer.string(name)?;
        writer.string(value)?;
    }
    Ok(())
}

pub(super) fn decode_form(reader: &mut WireReader<'_>) -> Result<FormSpec, ProtocolError> {
    let node_id = decode_node_id(reader)?;
    let action = reader.string(MAX_URL_BYTES)?;
    let method = reader.string(32)?;
    let count = bounded_count(reader.u32()?, MAX_FORM_FIELDS, "form fields")?;
    let mut hidden_fields = Vec::with_capacity(count);
    for _ in 0..count {
        hidden_fields.push((
            reader.string(MAX_CONTROL_TEXT_BYTES)?,
            reader.string(MAX_CONTROL_TEXT_BYTES)?,
        ));
    }
    Ok(FormSpec {
        node_id,
        action,
        method,
        hidden_fields,
    })
}

fn control_kind_tag(kind: ControlKind) -> u8 {
    match kind {
        ControlKind::Text => 1,
        ControlKind::TextArea => 2,
        ControlKind::Password => 3,
        ControlKind::Search => 4,
        ControlKind::Select => 5,
        ControlKind::Submit => 6,
        ControlKind::Button => 7,
        ControlKind::Reset => 8,
    }
}

fn decode_control_kind(tag: u8) -> Result<ControlKind, ProtocolError> {
    match tag {
        1 => Ok(ControlKind::Text),
        2 => Ok(ControlKind::TextArea),
        3 => Ok(ControlKind::Password),
        4 => Ok(ControlKind::Search),
        5 => Ok(ControlKind::Select),
        6 => Ok(ControlKind::Submit),
        7 => Ok(ControlKind::Button),
        8 => Ok(ControlKind::Reset),
        _ => Err(ProtocolError::InvalidPayload("control kind")),
    }
}
