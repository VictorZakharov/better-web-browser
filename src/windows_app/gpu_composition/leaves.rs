use super::*;
use std::collections::HashMap;

impl Compositor {
    pub(super) fn paint_leaf(
        &mut self,
        item: &DisplayItem,
        images: &HashMap<String, DecodedImage>,
        glyphs: &HashMap<u32, better_web_browser::renderer_protocol::PresentedGlyphRaster>,
        scale: f32,
        scroll: i32,
        groups: &groups::Groups,
    ) -> Result<(), String> {
        match item {
            DisplayItem::SolidRect {
                rect,
                color,
                radius,
            } => {
                if groups.visible(primitives::rect(*rect, scroll, scale)) {
                    if *radius > 0.0 {
                        shapes::paint(
                            self,
                            primitives::rect(*rect, scroll, scale),
                            *color,
                            *radius * scale,
                            None,
                        )?;
                    } else {
                        primitives::solid(
                            &self.device,
                            primitives::rect(*rect, scroll, scale),
                            *color,
                            *radius * scale,
                        )?;
                    }
                }
            }
            DisplayItem::BorderRect {
                rect,
                widths,
                colors,
                radius,
            } => {
                if groups.visible(primitives::rect(*rect, scroll, scale)) {
                    shapes::paint_border(
                        self,
                        primitives::rect(*rect, scroll, scale),
                        *colors,
                        widths.map(|width| width * scale),
                        *radius * scale,
                    )?;
                }
            }
            DisplayItem::Image {
                rect,
                clip,
                url,
                tint,
                ..
            } => {
                if !groups.visible(primitives::rect(clip.unwrap_or(*rect), scroll, scale)) {
                    return Ok(());
                }
                let bitmap = if let Some(bitmap) = self.frames.bitmap(url) {
                    if tint.is_some() || !groups.axis_aligned() {
                        return Err("GPU Canvas placement requires bitmap fallback".into());
                    }
                    Some(bitmap.clone())
                } else {
                    images
                        .get(url)
                        .map(|image| {
                            self.images.get(
                                &self.device,
                                url,
                                image,
                                tint.map(|color| [color.red, color.green, color.blue, color.alpha]),
                            )
                        })
                        .transpose()?
                };
                if let Some(bitmap) = bitmap {
                    if let Some(clip) = clip {
                        unsafe {
                            self.device.d2d.PushAxisAlignedClip(
                                &primitives::rect(*clip, scroll, scale),
                                D2D1_ANTIALIAS_MODE_ALIASED,
                            );
                        }
                    }
                    primitives::draw(
                        &self.device,
                        &bitmap,
                        primitives::image_rect(*rect, scroll, scale),
                    );
                    if clip.is_some() {
                        unsafe {
                            self.device.d2d.PopAxisAlignedClip();
                        }
                    }
                }
            }
            DisplayItem::Text {
                rect,
                color,
                glyphs: run,
                ..
            } => {
                for glyph in run {
                    let Some(resource) = glyphs
                        .get(&glyph.raster_id)
                        .filter(|resource| resource.color == glyph.color)
                    else {
                        continue;
                    };
                    // Glyph rasters already have final physical DPI. Match the
                    // native run's independent placement rounding and draw 1:1.
                    let left = (rect.x * scale).round() + (glyph.x * scale).round();
                    let top = (rect.y * scale).round() + (glyph.y * scale).round() - scroll as f32;
                    let destination = D2D_RECT_F {
                        left,
                        top,
                        right: left + resource.image.width as f32,
                        bottom: top + resource.image.height as f32,
                    };
                    if !groups.visible(destination) {
                        continue;
                    }
                    let tint = (!resource.color).then_some([
                        color.red,
                        color.green,
                        color.blue,
                        color.alpha,
                    ]);
                    let bitmap = self.images.get(
                        &self.device,
                        &format!("glyph:{}", glyph.raster_id),
                        &resource.image,
                        tint,
                    )?;
                    primitives::draw(&self.device, &bitmap, destination);
                }
            }
            DisplayItem::BackgroundImage {
                clip_rect,
                tile_rect,
                url,
                repeat_x,
                repeat_y,
            } => {
                if groups.visible(primitives::rect(*clip_rect, scroll, scale)) {
                    self.paint_background(
                        images, *clip_rect, *tile_rect, url, *repeat_x, *repeat_y, scale, scroll,
                    )?;
                }
            }
            DisplayItem::Control(spec) => {
                if !spec.authored_content
                    && !spec.kind.is_checkable()
                    && groups.visible(primitives::rect(spec.rect, scroll, scale))
                {
                    return Err("GPU native control content requires bitmap fallback".into());
                }
            }
            DisplayItem::PaintBoundary { .. }
            | DisplayItem::NodeBoundary { .. }
            | DisplayItem::CssGradient { .. }
            | DisplayItem::CssBoxShadow { .. }
            | DisplayItem::BeginCssMask { .. }
            | DisplayItem::EmbeddedFrame { .. } => {}
            _ => return Err("unhandled GPU page paint group".into()),
        }
        Ok(())
    }
}
