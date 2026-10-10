//! Checkbox/radio appearance is renderer-owned ink, not a native text-entry child.
//! Geometry, checkedness, activation and accessibility remain in the DOM/layout.
use super::*;
use crate::engine::css::Color;
use crate::engine::{ControlKind, ControlSpec};
#[cfg(test)]
mod tests;

pub(super) fn descriptor(item: &DisplayItem) -> Option<String> {
    let DisplayItem::Control(spec) = item else {
        return None;
    };
    spec.checkable
        .filter(|state| state.appearance)
        .map(|state| {
            format!(
                "checkable:{:?}:{:?}:{state:?}",
                spec.kind,
                (spec.rect.width, spec.rect.height),
            )
        })
}

pub(super) fn raster(spec: &ControlSpec, width: u32, height: u32) -> Option<DecodedImage> {
    let state = spec.checkable.filter(|state| state.appearance)?;
    let mut pixels = Pixmap::new(width, height)?;
    let side = spec.rect.width.min(spec.rect.height);
    let x = (spec.rect.width - side) / 2.0;
    let y = (spec.rect.height - side) / 2.0;
    let selected = state.checked || state.indeterminate;
    let accent = state.accent.unwrap_or(Color::rgb(0, 117, 255));
    let background = if state.disabled {
        if selected {
            Color::rgb(197, 197, 197)
        } else {
            Color::rgb(239, 239, 239)
        }
    } else if selected {
        accent
    } else {
        Color::WHITE
    };
    let border = if state.disabled {
        Color::rgb(197, 197, 197)
    } else if selected {
        accent
    } else {
        Color::rgb(118, 118, 118)
    };
    let radio = spec.kind == ControlKind::Radio;
    let outer = if radio {
        skia::PathBuilder::from_circle(x + side / 2.0, y + side / 2.0, side / 2.0)?
    } else {
        translated_round_rect(x, y, side, side, (side / 6.5).min(3.0))?
    };
    let transform = Transform::from_scale(
        width as f32 / spec.rect.width,
        height as f32 / spec.rect.height,
    );
    pixels.fill_path(
        &outer,
        &paint(background),
        skia::FillRule::Winding,
        transform,
        None,
    );
    let stroke = skia::Stroke {
        width: (side / 13.0).max(0.5),
        ..Default::default()
    };
    let inset = stroke.width / 2.0;
    let edge = if radio {
        skia::PathBuilder::from_circle(
            x + side / 2.0,
            y + side / 2.0,
            (side / 2.0 - inset).max(0.0),
        )?
    } else {
        translated_round_rect(
            x + inset,
            y + inset,
            side - inset * 2.0,
            side - inset * 2.0,
            (side / 6.5).min(3.0),
        )?
    };
    pixels.stroke_path(&edge, &paint(border), &stroke, transform, None);
    if selected {
        let foreground = if state.disabled {
            Color::WHITE
        } else if u32::from(accent.red) * 299
            + u32::from(accent.green) * 587
            + u32::from(accent.blue) * 114
            > 150_000
        {
            Color::BLACK
        } else {
            Color::WHITE
        };
        if radio {
            let dot = PathBuilder::from_circle(x + side / 2.0, y + side / 2.0, side * 0.23)?;
            pixels.fill_path(
                &dot,
                &paint(foreground),
                skia::FillRule::Winding,
                transform,
                None,
            );
        } else {
            let mut mark = PathBuilder::new();
            if state.indeterminate {
                mark.move_to(x + side * 0.23, y + side * 0.5);
                mark.line_to(x + side * 0.77, y + side * 0.5);
            } else {
                mark.move_to(x + side * 0.22, y + side * 0.5);
                mark.line_to(x + side * 0.43, y + side * 0.72);
                mark.line_to(x + side * 0.8, y + side * 0.25);
            }
            pixels.stroke_path(
                &mark.finish()?,
                &paint(foreground),
                &skia::Stroke {
                    width: side * 0.14,
                    line_cap: skia::LineCap::Round,
                    line_join: skia::LineJoin::Round,
                    ..Default::default()
                },
                transform,
                None,
            );
        }
    }
    let mut bgra = pixels.take();
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Some(DecodedImage {
        width,
        height,
        bgra: bgra.into(),
    })
}

fn paint(color: Color) -> Paint<'static> {
    let mut paint = Paint {
        anti_alias: true,
        ..Default::default()
    };
    paint.set_color_rgba8(color.red, color.green, color.blue, color.alpha);
    paint
}

fn translated_round_rect(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radius: f32,
) -> Option<skia::Path> {
    rounded_rect(width, height, radius)?.transform(Transform::from_translate(x, y))
}
