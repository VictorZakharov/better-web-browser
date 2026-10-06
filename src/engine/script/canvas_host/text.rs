//! Canvas text uses the page renderer's OpenType shaper, not synthetic glyph boxes.
//! Font bitmaps stay inside the renderer process and are bounded before V8 receives them.

use super::*;
use crate::engine::FontSpec;
use crate::engine::font::shaping::{FontCatalog, ShapeOptions, TextShaper};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use swash::scale::{Render, ScaleContext, Source, StrikeWith, image::Content};
use swash::zeno::{Angle, Format, Transform};

const MAX_TEXT_BYTES: usize = 16 * 1024;
const MAX_TEXT_GLYPHS: usize = 4096;
const MAX_TEXT_RASTER_BYTES: usize = 16 * 1024 * 1024;
mod glyph_cache;
mod run_cache;

thread_local! {
    static FONTS: RefCell<CanvasTextFonts> = RefCell::new(CanvasTextFonts::new());
}

pub(in crate::engine::script) struct CanvasTextFonts {
    catalog: FontCatalog,
    shaper: TextShaper,
    scale: ScaleContext,
    font_keys: HashMap<(u64, u32), swash::CacheKey>,
    glyph_cache: glyph_cache::GlyphCache,
    run_cache: run_cache::RunCache,
}

impl CanvasTextFonts {
    fn new() -> Self {
        Self {
            catalog: FontCatalog::new(),
            shaper: TextShaper::default(),
            scale: ScaleContext::new(),
            font_keys: HashMap::new(),
            glyph_cache: Default::default(),
            run_cache: Default::default(),
        }
    }

    fn run(
        &mut self,
        text: &str,
        spec: &FontSpec,
        stroke_width: Option<f32>,
        options: &ShapeOptions,
    ) -> JsValue {
        let key = run_cache::RunKey::new(text, spec, stroke_width, options);
        if let Some(value) = self.run_cache.get(&key) {
            return value;
        }
        let shaped = self
            .shaper
            .shape_with_options(&mut self.catalog, text, spec, options);
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
                // Whitespace can have a shaped advance but no rasterized ink.
                // Do not turn its placement origin into a text bounding box.
                if width == 0 || height == 0 {
                    continue;
                }
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
                    Content::Mask if image.data.len() == pixels => image.data.clone(),
                    Content::Color if image.data.len() == pixels * 4 => image.data.clone(),
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
        let value = JsValue::Array(vec![
            JsValue::from(f64::from(shaped.width)),
            JsValue::from(f64::from(baseline)),
            JsValue::from(f64::from((shaped.height - baseline).max(0.0))),
            JsValue::Array(glyphs),
        ]);
        self.run_cache.insert(key, &value);
        value
    }

    fn raster(
        &mut self,
        selected: &crate::engine::font::shaping::SelectedFont,
        glyph_id: u16,
        size: f32,
        stroke_width: Option<f32>,
    ) -> Option<Rc<swash::scale::image::Image>> {
        // FontInstanceKey contains the blob/face identity and the selection's
        // weight/italic synthesis. A registry change clears the entire cache.
        let key = glyph_cache::GlyphKey {
            font: selected.instance,
            glyph: glyph_id,
            size_bits: size.to_bits(),
            stroke_bits: stroke_width.map(f32::to_bits),
        };
        if let Some(image) = self.glyph_cache.get(&key) {
            return Some(image);
        }
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
        let image = Rc::new(render.render(&mut scaler, glyph_id)?);
        // Validate raster dimensions before retaining provider output, not only
        // before exporting it to JavaScript.
        let pixels = u64::from(image.placement.width) * u64::from(image.placement.height);
        if image.placement.width > crate::limits::MAX_GLYPH_RASTER_DIMENSION
            || image.placement.height > crate::limits::MAX_GLYPH_RASTER_DIMENSION
            || pixels > crate::limits::MAX_GLYPH_RASTER_PIXELS
        {
            return None;
        }
        self.glyph_cache.insert(key, image.clone());
        Some(image)
    }
}

fn font_spec(value: Option<&JsValue>) -> Option<FontSpec> {
    let JsValue::Array(parts) = value? else {
        return None;
    };
    if !matches!(parts.len(), 4 | 6) {
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
    let spacing = |index| -> Option<f32> {
        let value = parts.get(index).map_or(Some(0.0), JsValue::as_number)?;
        (value.is_finite() && value.abs() <= 1_000_000.0).then_some(value as f32)
    };
    Some(FontSpec {
        family,
        size: size as f32,
        weight: weight as u16,
        italic: parts[3].as_boolean()?,
        underline: false,
        letter_spacing: spacing(4)?,
        word_spacing: spacing(5)?,
        rtl: false,
        kerning: true,
        variants: Default::default(),
        features: Default::default(),
    })
}

pub(super) fn dispatch(operation: &str, args: &[JsValue]) -> JsResult<Option<JsValue>> {
    dispatch_with(operation, args, |text, spec, stroke, options| {
        FONTS.with_borrow_mut(|fonts| fonts.run(text, spec, stroke, options))
    })
}

pub(in crate::engine::script) fn dispatch_owned(
    operation: &str,
    args: &[JsValue],
    owned: &mut Option<CanvasTextFonts>,
    web_fonts: &[crate::engine::WebFont],
) -> JsResult<Option<JsValue>> {
    dispatch_with(operation, args, |text, spec, stroke, options| {
        let fonts = owned.get_or_insert_with(CanvasTextFonts::new);
        if fonts.catalog.register_web_fonts(web_fonts) {
            fonts.shaper.clear();
            fonts.font_keys.clear();
            fonts.glyph_cache.clear();
            fonts.run_cache.clear();
            fonts.scale = ScaleContext::new();
        }
        fonts.run(text, spec, stroke, options)
    })
}

fn dispatch_with(
    operation: &str,
    args: &[JsValue],
    run: impl FnOnce(&str, &FontSpec, Option<f32>, &ShapeOptions) -> JsValue,
) -> JsResult<Option<JsValue>> {
    if operation == "canvasParseSpacing" {
        let value = args.get(1).map(JsValue::string_value).unwrap_or_default();
        return Ok(Some(
            crate::engine::css::font_family::spacing::parse(&value)
                .map(|(serialized, terms)| {
                    JsValue::Array(vec![
                        JsValue::from(serialized),
                        JsValue::Array(
                            terms
                                .into_iter()
                                .map(|term| JsValue::from(f64::from(term)))
                                .collect(),
                        ),
                    ])
                })
                .unwrap_or(JsValue::Null),
        ));
    }
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
    let options = shaping_options(args.get(4)).ok_or_else(|| {
        JsNativeError::typ().with_message("Canvas text requires valid shaping options")
    })?;
    Ok(Some(run(&text, &spec, stroke_width, &options)))
}

fn shaping_options(value: Option<&JsValue>) -> Option<ShapeOptions> {
    let Some(value) = value else {
        return Some(ShapeOptions::default());
    };
    let JsValue::Array(parts) = value else {
        return None;
    };
    if parts.len() != 3 {
        return None;
    }
    let JsValue::String(direction) = &parts[0] else {
        return None;
    };
    let rtl = match direction.as_str() {
        "ltr" => false,
        "rtl" => true,
        _ => return None,
    };
    let JsValue::String(language) = &parts[1] else {
        return None;
    };
    if language.len() > 256 {
        return None;
    }
    let JsValue::String(kerning) = &parts[2] else {
        return None;
    };
    let kerning = match kerning.as_str() {
        "auto" | "normal" => true,
        "none" => false,
        _ => return None,
    };
    Some(ShapeOptions {
        rtl: Some(rtl),
        language: language.parse().ok(),
        kerning,
    })
}
