//! Canvas text uses the page renderer's OpenType shaper, not synthetic glyph boxes.
//! Font bitmaps stay inside the renderer process and are bounded before V8 receives them.

use super::*;
use crate::engine::FontSpec;
use crate::engine::font::shaping::{FontCatalog, TextShaper};
use std::cell::RefCell;
use std::collections::HashMap;
use swash::scale::{Render, ScaleContext, Source, StrikeWith, image::Content};
use swash::zeno::{Angle, Format, Transform};

const MAX_TEXT_BYTES: usize = 16 * 1024;
const MAX_TEXT_GLYPHS: usize = 4096;
const MAX_TEXT_RASTER_BYTES: usize = 16 * 1024 * 1024;

thread_local! {
    static FONTS: RefCell<CanvasTextFonts> = RefCell::new(CanvasTextFonts::new());
}

struct CanvasTextFonts {
    catalog: FontCatalog,
    shaper: TextShaper,
    scale: ScaleContext,
    font_keys: HashMap<(u64, u32), swash::CacheKey>,
}

impl CanvasTextFonts {
    fn new() -> Self {
        Self {
            catalog: FontCatalog::new(),
            shaper: TextShaper::default(),
            scale: ScaleContext::new(),
            font_keys: HashMap::new(),
        }
    }

    fn run(&mut self, text: &str, spec: &FontSpec, stroke_width: Option<f32>) -> JsValue {
        let shaped = self.shaper.shape(&mut self.catalog, text, spec);
        let baseline = shaped
            .glyphs
            .first()
            .map_or(spec.size * 0.9, |glyph| glyph.baseline);
        let mut glyphs = Vec::new();
        let mut bytes = 0usize;
        if shaped.glyphs.len() <= MAX_TEXT_GLYPHS {
            for glyph in shaped.glyphs {
                let Some(image) = self.raster(&glyph.font, glyph.glyph_id, spec.size, stroke_width)
                else {
                    continue;
                };
                let width = image.placement.width;
                let height = image.placement.height;
                let Some(pixels) = usize::try_from(u64::from(width) * u64::from(height)).ok()
                else {
                    continue;
                };
                if width > crate::limits::MAX_GLYPH_RASTER_DIMENSION
                    || height > crate::limits::MAX_GLYPH_RASTER_DIMENSION
                    || pixels > crate::limits::MAX_GLYPH_RASTER_PIXELS as usize
                {
                    continue;
                }
                let data = match image.content {
                    Content::Mask if image.data.len() == pixels => image.data,
                    Content::Color if image.data.len() == pixels * 4 => image.data,
                    Content::SubpixelMask if image.data.len() == pixels * 3 => image
                        .data
                        .chunks_exact(3)
                        .map(|rgb| rgb.iter().copied().max().unwrap_or(0))
                        .collect(),
                    _ => continue,
                };
                bytes = bytes.saturating_add(data.len());
                if bytes > MAX_TEXT_RASTER_BYTES {
                    break;
                }
                glyphs.push(JsValue::Array(vec![
                    JsValue::from(f64::from(glyph.x) + f64::from(image.placement.left)),
                    JsValue::from(
                        f64::from(glyph.baseline - baseline) - f64::from(image.placement.top),
                    ),
                    JsValue::from(width),
                    JsValue::from(height),
                    JsValue::from(image.content == Content::Color),
                    JsValue::Bytes(data),
                ]));
            }
        }
        JsValue::Array(vec![
            JsValue::from(f64::from(shaped.width)),
            JsValue::from(f64::from(baseline)),
            JsValue::from(f64::from((shaped.height - baseline).max(0.0))),
            JsValue::Array(glyphs),
        ])
    }

    fn raster(
        &mut self,
        selected: &crate::engine::font::shaping::SelectedFont,
        glyph_id: u16,
        size: f32,
        stroke_width: Option<f32>,
    ) -> Option<swash::scale::image::Image> {
        let mut font =
            swash::FontRef::from_index(selected.font.blob.as_ref(), selected.font.index as usize)?;
        font.key = *self
            .font_keys
            .entry((selected.font.blob.id(), selected.font.index))
            .or_insert(font.key);
        let settings = selected
            .font
            .synthesis
            .variation_settings()
            .iter()
            .map(|(tag, value)| (swash::Tag::from_be_bytes(tag.to_be_bytes()), *value));
        let coords = font
            .variations()
            .normalized_coords(settings)
            .collect::<Vec<_>>();
        let mut builder = self.scale.builder(font).size(size).hint(true);
        if !coords.is_empty() {
            builder = builder.normalized_coords(&coords);
        }
        let mut scaler = builder.build();
        let transform =
            selected.font.synthesis.skew().map(|degrees| {
                Transform::skew(Angle::from_degrees(degrees), Angle::from_degrees(0.0))
            });
        let fill_sources = [
            Source::ColorOutline(0),
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::Outline,
        ];
        let stroke_sources = [Source::Outline];
        let mut render = Render::new(if stroke_width.is_some() {
            &stroke_sources
        } else {
            &fill_sources
        });
        render.format(Format::Alpha).transform(transform).embolden(
            if selected.font.synthesis.embolden() {
                size * 0.02
            } else {
                0.0
            },
        );
        if let Some(width) = stroke_width {
            render.style(swash::zeno::Stroke::new(width));
        }
        render.render(&mut scaler, glyph_id)
    }
}

fn font_spec(value: Option<&JsValue>) -> Option<FontSpec> {
    let JsValue::Array(parts) = value? else {
        return None;
    };
    if parts.len() != 4 {
        return None;
    }
    let size = parts[1].as_number()?;
    let weight = parts[2].as_number()?;
    if !size.is_finite()
        || !(1.0..=768.0).contains(&size)
        || !weight.is_finite()
        || !(1.0..=1000.0).contains(&weight)
    {
        return None;
    }
    let family = parts[0].string_value();
    if family.len() > 1024 {
        return None;
    }
    Some(FontSpec {
        family,
        size: size as f32,
        weight: weight as u16,
        italic: parts[3].as_boolean()?,
        underline: false,
        letter_spacing: 0.0,
        word_spacing: 0.0,
    })
}

pub(super) fn dispatch(operation: &str, args: &[JsValue]) -> JsResult<Option<JsValue>> {
    if operation == "canvasParseFont" {
        let value = args.get(1).map(JsValue::string_value).unwrap_or_default();
        let result = crate::engine::css::font_family::parse_canvas_font(&value)
            .map(|spec| {
                JsValue::Array(vec![
                    JsValue::from(spec.family),
                    JsValue::from(f64::from(spec.size)),
                    JsValue::from(f64::from(spec.weight)),
                    JsValue::from(spec.italic),
                ])
            })
            .unwrap_or(JsValue::Null);
        return Ok(Some(result));
    }
    if !matches!(
        operation,
        "canvasMeasureText" | "canvasRasterText" | "canvasStrokeText"
    ) {
        return Ok(None);
    }
    let text = args.get(1).map(JsValue::string_value).unwrap_or_default();
    if text.len() > MAX_TEXT_BYTES {
        return Err(JsNativeError::range()
            .with_message("Canvas text exceeds the text budget")
            .into());
    }
    let spec = font_spec(args.get(2))
        .ok_or_else(|| JsNativeError::typ().with_message("Canvas text requires a valid font"))?;
    let stroke_width = if operation == "canvasStrokeText" {
        let width = args.get(3).and_then(JsValue::as_number).unwrap_or(1.0);
        Some(width.clamp(0.01, 128.0) as f32)
    } else {
        None
    };
    Ok(Some(FONTS.with_borrow_mut(|fonts| {
        fonts.run(&text, &spec, stroke_width)
    })))
}
