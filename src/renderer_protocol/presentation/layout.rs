use super::super::wire::{WireReader, WireWriter};
use super::*;
use crate::engine::css::Color;
use crate::engine::dom::NodeId;
use crate::engine::{
    ControlKind, ControlSpec, DisplayItem, FontSpec, FormSpec, PositionedGlyph, RectF, SelectOption,
};
use crate::limits::{
    MAX_DOM_NODES, MAX_PRESENTATION_COORDINATE, MAX_RENDERED_TEXT_BYTES, MAX_URL_BYTES,
};

mod controls;
mod groups;
pub(super) mod sticky;
mod values;
use controls::{decode_control, decode_form, encode_control, encode_form};
use groups::validate_display_groups;
use values::*;

const MAX_CONTROL_OPTIONS: usize = 10_000;
const MAX_FORM_FIELDS: usize = 10_000;
const MAX_FAMILY_BYTES: usize = 1024;
const MAX_CONTROL_TEXT_BYTES: usize = 64 * 1024;
const MAX_GLYPHS_PER_TEXT_ITEM: usize = 65_536;

pub(super) fn encode_layout(
    writer: &mut WireWriter,
    layout: &PresentedLayout,
) -> Result<(), ProtocolError> {
    validate_display_groups(&layout.items)?;
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
    sticky::encode(writer, &layout.sticky_layers, layout.items.len())?;
    Ok(())
}

pub(super) fn decode_layout(reader: &mut WireReader<'_>) -> Result<PresentedLayout, ProtocolError> {
    let content_height = finite(
        reader.f32()?,
        0.0,
        MAX_PRESENTATION_COORDINATE,
        "content height",
    )?;
    let background = decode_color(reader)?;
    let item_count = bounded_count(reader.u32()?, MAX_DOM_NODES * 4, "display items")?;
    let mut items = Vec::with_capacity(item_count);
    for _ in 0..item_count {
        items.push(decode_item(reader)?);
    }
    validate_display_groups(&items)?;
    let form_count = bounded_count(reader.u32()?, MAX_DOM_NODES, "forms")?;
    let mut forms = Vec::with_capacity(form_count);
    for _ in 0..form_count {
        forms.push(decode_form(reader)?);
    }
    let sticky_layers = sticky::decode(reader, item_count)?;
    Ok(PresentedLayout {
        items,
        sticky_layers,
        content_height,
        background,
        forms,
    })
}

fn encode_item(writer: &mut WireWriter, item: &DisplayItem) -> Result<(), ProtocolError> {
    match item {
        DisplayItem::NodeBoundary { .. }
        | DisplayItem::PaintBoundary { .. }
        | DisplayItem::EmbeddedFrame { .. } => {
            return Err(ProtocolError::InvalidPayload("renderer-local paint anchor"));
        }
        DisplayItem::BeginClip { bounds } => {
            writer.u8(9);
            encode_rect(writer, *bounds);
        }
        DisplayItem::EndClip { bounds } => {
            writer.u8(10);
            encode_rect(writer, *bounds);
        }
        DisplayItem::BeginOpacity { bounds, opacity } => {
            writer.u8(7);
            encode_rect(writer, *bounds);
            writer.f32(*opacity);
        }
        DisplayItem::EndOpacity { bounds } => {
            writer.u8(8);
            encode_rect(writer, *bounds);
        }
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
            colors,
            radius,
        } => {
            writer.u8(2);
            encode_rect(writer, *rect);
            encode_edges(writer, *widths);
            for color in colors {
                encode_color(writer, *color);
            }
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
            clip,
            url,
            alt,
            tint,
        } => {
            writer.u8(4);
            encode_rect(writer, *rect);
            writer.bool(clip.is_some());
            if let Some(clip) = clip {
                encode_rect(writer, *clip);
            }
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
        9 => Ok(DisplayItem::BeginClip {
            bounds: decode_rect(reader)?,
        }),
        10 => Ok(DisplayItem::EndClip {
            bounds: decode_rect(reader)?,
        }),
        7 => Ok(DisplayItem::BeginOpacity {
            bounds: decode_rect(reader)?,
            opacity: finite(reader.f32()?, 0.0, 1.0, "opacity")?,
        }),
        8 => Ok(DisplayItem::EndOpacity {
            bounds: decode_rect(reader)?,
        }),
        1 => Ok(DisplayItem::SolidRect {
            rect: decode_rect(reader)?,
            color: decode_color(reader)?,
            radius: finite(reader.f32()?, 0.0, MAX_PRESENTATION_COORDINATE, "radius")?,
        }),
        2 => Ok(DisplayItem::BorderRect {
            rect: decode_rect(reader)?,
            widths: decode_edges(reader)?,
            colors: [
                decode_color(reader)?,
                decode_color(reader)?,
                decode_color(reader)?,
                decode_color(reader)?,
            ],
            radius: finite(reader.f32()?, 0.0, MAX_PRESENTATION_COORDINATE, "radius")?,
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
                    x: finite(
                        reader.f32()?,
                        -MAX_PRESENTATION_COORDINATE,
                        MAX_PRESENTATION_COORDINATE,
                        "glyph x",
                    )?,
                    y: finite(
                        reader.f32()?,
                        -MAX_PRESENTATION_COORDINATE,
                        MAX_PRESENTATION_COORDINATE,
                        "glyph y",
                    )?,
                    width: finite(
                        reader.f32()?,
                        0.0,
                        MAX_PRESENTATION_COORDINATE,
                        "glyph width",
                    )?,
                    height: finite(
                        reader.f32()?,
                        0.0,
                        MAX_PRESENTATION_COORDINATE,
                        "glyph height",
                    )?,
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
            clip: reader.bool()?.then(|| decode_rect(reader)).transpose()?,
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
