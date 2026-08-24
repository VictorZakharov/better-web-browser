use super::super::wire::{WireReader, WireWriter};
use super::*;
use crate::engine::css::Color;
use crate::engine::dom::NodeId;
use crate::engine::{
    ControlKind, ControlSpec, DisplayItem, FontSpec, FormSpec, PositionedGlyph, RectF, SelectOption,
};
use crate::limits::{MAX_DOM_NODES, MAX_RENDERED_TEXT_BYTES, MAX_URL_BYTES};

const MAX_CONTROL_OPTIONS: usize = 10_000;
const MAX_FORM_FIELDS: usize = 10_000;
const MAX_FAMILY_BYTES: usize = 1024;
const MAX_CONTROL_TEXT_BYTES: usize = 64 * 1024;
const MAX_COORDINATE: f32 = 10_000_000.0;
const MAX_GLYPHS_PER_TEXT_ITEM: usize = 65_536;

pub(super) fn encode_layout(
    writer: &mut WireWriter,
    layout: &PresentedLayout,
) -> Result<(), ProtocolError> {
    writer.f32(layout.content_height);
    encode_color(writer, layout.background);
    writer.u32(layout.items.len() as u32);
    for item in &layout.items {
        encode_item(writer, item)?;
    }
    writer.u32(layout.forms.len() as u32);
    for form in &layout.forms {
        encode_form(writer, form)?;
    }
    Ok(())
}

pub(super) fn decode_layout(reader: &mut WireReader<'_>) -> Result<PresentedLayout, ProtocolError> {
    let content_height = finite(reader.f32()?, 0.0, MAX_COORDINATE, "content height")?;
    let background = decode_color(reader)?;
    let item_count = bounded_count(reader.u32()?, MAX_DOM_NODES * 4, "display items")?;
    let mut items = Vec::with_capacity(item_count);
    for _ in 0..item_count {
        items.push(decode_item(reader)?);
    }
    let form_count = bounded_count(reader.u32()?, MAX_DOM_NODES, "forms")?;
    let mut forms = Vec::with_capacity(form_count);
    for _ in 0..form_count {
        forms.push(decode_form(reader)?);
    }
    Ok(PresentedLayout {
        items,
        content_height,
        background,
        forms,
    })
}

fn encode_item(writer: &mut WireWriter, item: &DisplayItem) -> Result<(), ProtocolError> {
    match item {
        DisplayItem::SolidRect {
            rect,
            color,
            radius,
        } => {
            writer.u8(1);
            encode_rect(writer, *rect);
            encode_color(writer, *color);
            writer.f32(*radius);
        }
        DisplayItem::BorderRect {
            rect,
            widths,
            color,
            radius,
        } => {
            writer.u8(2);
            encode_rect(writer, *rect);
            encode_edges(writer, *widths);
            encode_color(writer, *color);
            writer.f32(*radius);
        }
        DisplayItem::Text {
            rect,
            text,
            font,
            color,
            link,
            node_id,
            raster_run_id,
            glyphs,
        } => {
            writer.u8(3);
            encode_rect(writer, *rect);
            writer.string(text)?;
            encode_font(writer, font)?;
            encode_color(writer, *color);
            encode_optional_string(writer, link.as_deref())?;
            writer.bool(node_id.is_some());
            if let Some(node_id) = node_id {
                writer.u128(node_id.to_wire());
            }
            writer.u64(*raster_run_id);
            writer.u32(glyphs.len() as u32);
            for glyph in glyphs.iter() {
                writer.u32(glyph.raster_id);
                writer.f32(glyph.x);
                writer.f32(glyph.y);
                writer.f32(glyph.width);
                writer.f32(glyph.height);
                writer.bool(glyph.color);
            }
        }
        DisplayItem::Image {
            rect,
            url,
            alt,
            tint,
        } => {
            writer.u8(4);
            encode_rect(writer, *rect);
            writer.string(url)?;
            writer.string(alt)?;
            writer.bool(tint.is_some());
            if let Some(tint) = tint {
                encode_color(writer, *tint);
            }
        }
        DisplayItem::BackgroundImage {
            clip_rect,
            tile_rect,
            url,
            repeat_x,
            repeat_y,
        } => {
            writer.u8(5);
            encode_rect(writer, *clip_rect);
            encode_rect(writer, *tile_rect);
            writer.string(url)?;
            writer.bool(*repeat_x);
            writer.bool(*repeat_y);
        }
        DisplayItem::Control(spec) => {
            writer.u8(6);
            encode_control(writer, spec)?;
        }
    }
    Ok(())
}

fn decode_item(reader: &mut WireReader<'_>) -> Result<DisplayItem, ProtocolError> {
    match reader.u8()? {
        1 => Ok(DisplayItem::SolidRect {
            rect: decode_rect(reader)?,
            color: decode_color(reader)?,
            radius: finite(reader.f32()?, 0.0, MAX_COORDINATE, "radius")?,
        }),
        2 => Ok(DisplayItem::BorderRect {
            rect: decode_rect(reader)?,
            widths: decode_edges(reader)?,
            color: decode_color(reader)?,
            radius: finite(reader.f32()?, 0.0, MAX_COORDINATE, "radius")?,
        }),
        3 => {
            let rect = decode_rect(reader)?;
            let text = reader.string(MAX_RENDERED_TEXT_BYTES)?;
            let font = decode_font(reader)?;
            let color = decode_color(reader)?;
            let link = decode_optional_string(reader, MAX_URL_BYTES)?;
            let node_id = reader.bool()?.then(|| decode_node_id(reader)).transpose()?;
            let raster_run_id = reader.u64()?;
            let glyph_count =
                bounded_count(reader.u32()?, MAX_GLYPHS_PER_TEXT_ITEM, "positioned glyphs")?;
            if glyph_count > 0 && raster_run_id == 0 {
                return Err(ProtocolError::InvalidPayload("text raster run identifier"));
            }
            let mut glyphs = Vec::with_capacity(glyph_count);
            for _ in 0..glyph_count {
                glyphs.push(PositionedGlyph {
                    raster_id: reader.u32()?,
                    x: finite(reader.f32()?, -MAX_COORDINATE, MAX_COORDINATE, "glyph x")?,
                    y: finite(reader.f32()?, -MAX_COORDINATE, MAX_COORDINATE, "glyph y")?,
                    width: finite(reader.f32()?, 0.0, MAX_COORDINATE, "glyph width")?,
                    height: finite(reader.f32()?, 0.0, MAX_COORDINATE, "glyph height")?,
                    color: reader.bool()?,
                });
            }
            Ok(DisplayItem::Text {
                rect,
                text,
                font,
                color,
                link,
                node_id,
                raster_run_id,
                glyphs,
            })
        }
        4 => Ok(DisplayItem::Image {
            rect: decode_rect(reader)?,
            url: reader.string(MAX_URL_BYTES)?,
            alt: reader.string(MAX_RENDERED_TEXT_BYTES)?,
            tint: reader.bool()?.then(|| decode_color(reader)).transpose()?,
        }),
        5 => Ok(DisplayItem::BackgroundImage {
            clip_rect: decode_rect(reader)?,
            tile_rect: decode_rect(reader)?,
            url: reader.string(MAX_URL_BYTES)?,
            repeat_x: reader.bool()?,
            repeat_y: reader.bool()?,
        }),
        6 => Ok(DisplayItem::Control(Box::new(decode_control(reader)?))),
        _ => Err(ProtocolError::InvalidPayload("display item tag")),
    }
}

fn encode_control(writer: &mut WireWriter, spec: &ControlSpec) -> Result<(), ProtocolError> {
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

fn decode_control(reader: &mut WireReader<'_>) -> Result<ControlSpec, ProtocolError> {
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
        border_radius: finite(reader.f32()?, 0.0, MAX_COORDINATE, "control radius")?,
        padding: decode_edges(reader)?,
        font: decode_font(reader)?,
        icon_url: decode_optional_string(reader, MAX_URL_BYTES)?,
        icon_width: finite(reader.f32()?, 0.0, MAX_COORDINATE, "control icon width")?,
        icon_height: finite(reader.f32()?, 0.0, MAX_COORDINATE, "control icon height")?,
    })
}

fn encode_form(writer: &mut WireWriter, form: &FormSpec) -> Result<(), ProtocolError> {
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

fn decode_form(reader: &mut WireReader<'_>) -> Result<FormSpec, ProtocolError> {
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

fn encode_rect(writer: &mut WireWriter, rect: RectF) {
    writer.f32(rect.x);
    writer.f32(rect.y);
    writer.f32(rect.width);
    writer.f32(rect.height);
}

fn decode_rect(reader: &mut WireReader<'_>) -> Result<RectF, ProtocolError> {
    Ok(RectF {
        x: finite(reader.f32()?, -MAX_COORDINATE, MAX_COORDINATE, "rect x")?,
        y: finite(reader.f32()?, -MAX_COORDINATE, MAX_COORDINATE, "rect y")?,
        width: finite(reader.f32()?, 0.0, MAX_COORDINATE, "rect width")?,
        height: finite(reader.f32()?, 0.0, MAX_COORDINATE, "rect height")?,
    })
}

fn encode_font(writer: &mut WireWriter, font: &FontSpec) -> Result<(), ProtocolError> {
    writer.string(&font.family)?;
    writer.f32(font.size);
    writer.u16(font.weight);
    writer.bool(font.italic);
    writer.bool(font.underline);
    writer.f32(font.letter_spacing);
    writer.f32(font.word_spacing);
    Ok(())
}

fn decode_font(reader: &mut WireReader<'_>) -> Result<FontSpec, ProtocolError> {
    let family = reader.string(MAX_FAMILY_BYTES)?;
    let size = finite(reader.f32()?, 1.0, 768.0, "font size")?;
    let weight = reader.u16()?;
    if !(1..=1000).contains(&weight) {
        return Err(ProtocolError::InvalidPayload("font weight"));
    }
    Ok(FontSpec {
        family,
        size,
        weight,
        italic: reader.bool()?,
        underline: reader.bool()?,
        letter_spacing: finite(reader.f32()?, -768.0, 768.0, "letter spacing")?,
        word_spacing: finite(reader.f32()?, -768.0, 768.0, "word spacing")?,
    })
}

fn encode_edges(writer: &mut WireWriter, values: [f32; 4]) {
    for value in values {
        writer.f32(value);
    }
}

fn decode_edges(reader: &mut WireReader<'_>) -> Result<[f32; 4], ProtocolError> {
    Ok([
        finite(reader.f32()?, 0.0, MAX_COORDINATE, "edge")?,
        finite(reader.f32()?, 0.0, MAX_COORDINATE, "edge")?,
        finite(reader.f32()?, 0.0, MAX_COORDINATE, "edge")?,
        finite(reader.f32()?, 0.0, MAX_COORDINATE, "edge")?,
    ])
}

fn encode_color(writer: &mut WireWriter, color: Color) {
    writer.u8(color.red);
    writer.u8(color.green);
    writer.u8(color.blue);
    writer.u8(color.alpha);
}

fn decode_color(reader: &mut WireReader<'_>) -> Result<Color, ProtocolError> {
    Ok(Color {
        red: reader.u8()?,
        green: reader.u8()?,
        blue: reader.u8()?,
        alpha: reader.u8()?,
    })
}

fn encode_optional_string(
    writer: &mut WireWriter,
    value: Option<&str>,
) -> Result<(), ProtocolError> {
    writer.bool(value.is_some());
    if let Some(value) = value {
        writer.string(value)?;
    }
    Ok(())
}

fn decode_optional_string(
    reader: &mut WireReader<'_>,
    maximum: usize,
) -> Result<Option<String>, ProtocolError> {
    reader.bool()?.then(|| reader.string(maximum)).transpose()
}

fn decode_node_id(reader: &mut WireReader<'_>) -> Result<NodeId, ProtocolError> {
    NodeId::from_wire(reader.u128()?).ok_or(ProtocolError::InvalidPayload("node identifier"))
}

fn finite(
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

fn bounded_count(value: u32, maximum: usize, field: &'static str) -> Result<usize, ProtocolError> {
    let value = value as usize;
    (value <= maximum)
        .then_some(value)
        .ok_or(ProtocolError::InvalidPayload(field))
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
