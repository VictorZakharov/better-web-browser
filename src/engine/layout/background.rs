use super::*;
pub(super) mod shadows;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn background_paint_item(
        &self,
        style: &ComputedStyle,
        image: &ImageSource,
        layer: usize,
        rect: RectF,
        radius: f32,
    ) -> Option<DisplayItem> {
        // CSS Backgrounds 3: the initial positioning area is the padding box;
        // the initial painting area still extends underneath the border.
        let border = style.border_width.resolve(rect.width, style.font_size);
        let area = RectF {
            x: rect.x + border.left,
            y: rect.y + border.top,
            width: (rect.width - border.horizontal()).max(0.0),
            height: (rect.height - border.vertical()).max(0.0),
        };
        self.image_layer_item(style, image, layer, rect, area, radius, false)
    }
    pub(super) fn mask_paint_item(
        &self,
        style: &ComputedStyle,
        image: &ImageSource,
        layer: usize,
        rect: RectF,
    ) -> Option<DisplayItem> {
        // Initial mask-origin/mask-clip are border-box, unlike backgrounds.
        self.image_layer_item(style, image, layer, rect, rect, 0.0, true)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn image_layer_item(
        &self,
        style: &ComputedStyle,
        image: &ImageSource,
        layer: usize,
        rect: RectF,
        area: RectF,
        radius: f32,
        mask: bool,
    ) -> Option<DisplayItem> {
        let natural = match image {
            ImageSource::None => return None,
            ImageSource::Url(url) => {
                let image = self.decoded_image(url)?;
                (
                    image.width as f32 * style.effective_zoom,
                    image.height as f32 * style.effective_zoom,
                )
            }
            ImageSource::Gradient(_) => (area.width, area.height),
        };
        let (size, x, y, repeats) = if mask {
            style.mask_layers.at(layer)
        } else if layer == 0 {
            (
                style.background_size.clone(),
                style.background_position_x.clone(),
                style.background_position_y.clone(),
                (style.background_repeat_x, style.background_repeat_y),
            )
        } else {
            style.background_layers.at(layer)
        };
        let (width, height) = resolve_background_size(
            size,
            area,
            natural.0,
            natural.1,
            style.font_size,
            self.viewport,
        )?;
        let tile_rect = RectF {
            x: area.x
                + resolve_background_position(x, area.width, width, style.font_size, self.viewport),
            y: area.y
                + resolve_background_position(
                    y,
                    area.height,
                    height,
                    style.font_size,
                    self.viewport,
                ),
            width,
            height,
        };
        Some(match image {
            ImageSource::Url(url) => DisplayItem::BackgroundImage {
                clip_rect: rect,
                tile_rect,
                url: url.clone(),
                repeat_x: repeats.0,
                repeat_y: repeats.1,
            },
            ImageSource::Gradient(gradient) => DisplayItem::CssGradient {
                clip_rect: rect,
                tile_rect,
                gradient: gradient.resolve(
                    width,
                    height,
                    style.font_size,
                    style.effective_zoom,
                    style.color,
                )?,
                radius,
                repeat_x: repeats.0,
                repeat_y: repeats.1,
            },
            ImageSource::None => return None,
        })
    }
    pub(super) fn control_background_color(
        &self,
        node: &NodeRef,
        style: &ComputedStyle,
        kind: ControlKind,
    ) -> Color {
        // Keep CSS transparency in alpha, but carry the composited backdrop in RGB.
        // Native EDIT controls cannot reliably repaint with a hollow brush while focused;
        // the host uses this color for a stable opaque child surface and clips its HWND
        // to any rounded backdrop in the display list.
        if matches!(
            kind,
            ControlKind::Text | ControlKind::TextArea | ControlKind::Password | ControlKind::Search
        ) && style.background_color.alpha == 0
        {
            Color {
                alpha: 0,
                ..self.effective_background_color(node)
            }
        } else {
            self.effective_background_color(node)
        }
    }

    pub(super) fn background_tile_rect(
        &self,
        style: &ComputedStyle,
        clip_rect: RectF,
    ) -> Option<RectF> {
        let url = style.background_image.as_ref()?;
        let image = self.decoded_image(url)?;
        let (width, height) = resolve_background_size(
            style.background_size.clone(),
            clip_rect,
            image.width as f32 * style.effective_zoom,
            image.height as f32 * style.effective_zoom,
            style.font_size,
            self.viewport,
        )?;
        let x = clip_rect.x
            + resolve_background_position(
                style.background_position_x.clone(),
                clip_rect.width,
                width,
                style.font_size,
                self.viewport,
            );
        let y = clip_rect.y
            + resolve_background_position(
                style.background_position_y.clone(),
                clip_rect.height,
                height,
                style.font_size,
                self.viewport,
            );
        Some(RectF {
            x,
            y,
            width,
            height,
        })
    }

    pub(super) fn control_background_icon(
        &self,
        style: &ComputedStyle,
        width: f32,
        height: f32,
    ) -> Option<(String, f32, f32)> {
        if style.background_repeat_x || style.background_repeat_y {
            return None;
        }
        let tile = self.background_tile_rect(
            style,
            RectF {
                x: 0.0,
                y: 0.0,
                width,
                height,
            },
        )?;
        Some((
            style.background_image.as_ref()?.clone(),
            tile.width,
            tile.height,
        ))
    }
}

pub(super) fn resolve_background_size(
    size: BackgroundSize,
    area: RectF,
    natural_width: f32,
    natural_height: f32,
    font_size: f32,
    viewport: RectF,
) -> Option<(f32, f32)> {
    if natural_width <= 0.0 || natural_height <= 0.0 {
        return None;
    }
    let (width, height) = match size {
        BackgroundSize::Auto => (natural_width, natural_height),
        BackgroundSize::Contain | BackgroundSize::Cover => {
            let horizontal = area.width / natural_width;
            let vertical = area.height / natural_height;
            let scale = if size == BackgroundSize::Contain {
                horizontal.min(vertical)
            } else {
                horizontal.max(vertical)
            };
            (natural_width * scale, natural_height * scale)
        }
        BackgroundSize::Explicit { width, height } => {
            let width = resolve_background_length(width, area.width, font_size, viewport);
            let height = resolve_background_length(height, area.height, font_size, viewport);
            match (width, height) {
                (Some(width), Some(height)) => (width, height),
                (Some(width), None) => (width, width * natural_height / natural_width),
                (None, Some(height)) => (height * natural_width / natural_height, height),
                (None, None) => (natural_width, natural_height),
            }
        }
    };
    (width > 0.0 && height > 0.0).then_some((width, height))
}

pub(super) fn resolve_background_position(
    position: Length,
    area_size: f32,
    image_size: f32,
    font_size: f32,
    viewport: RectF,
) -> f32 {
    match position {
        Length::Percent(percent) => (area_size - image_size) * percent / 100.0,
        Length::Calc {
            px,
            percent,
            em,
            rem,
            vw,
            vh,
            vmin,
            vmax,
        } => {
            px + (area_size - image_size) * percent / 100.0
                + font_size * em
                + 16.0 * rem
                + viewport.width * vw / 100.0
                + viewport.height * vh / 100.0
                + viewport.width.min(viewport.height) * vmin / 100.0
                + viewport.width.max(viewport.height) * vmax / 100.0
        }
        _ => resolve_background_length(position, area_size, font_size, viewport).unwrap_or(0.0),
    }
}

pub(super) fn resolve_background_length(
    length: Length,
    basis: f32,
    font_size: f32,
    viewport: RectF,
) -> Option<f32> {
    match length {
        Length::Math(value) => value
            .map_lengths(&|length| {
                length
                    .clone()
                    .resolve_viewport_units(viewport.width, viewport.height)
            })
            .resolve(Some(basis), font_size),
        Length::Auto => None,
        Length::Px(value) => Some(value),
        Length::Percent(value) => Some(basis * value / 100.0),
        Length::Em(value) => Some(font_size * value),
        Length::Rem(value) => Some(16.0 * value),
        Length::Vw(value) => Some(viewport.width * value / 100.0),
        Length::Vh(value) => Some(viewport.height * value / 100.0),
        Length::Vmin(value) => Some(viewport.width.min(viewport.height) * value / 100.0),
        Length::Vmax(value) => Some(viewport.width.max(viewport.height) * value / 100.0),
        Length::Calc {
            px,
            percent,
            em,
            rem,
            vw,
            vh,
            vmin,
            vmax,
        } => Some(
            px + basis * percent / 100.0
                + font_size * em
                + 16.0 * rem
                + viewport.width * vw / 100.0
                + viewport.height * vh / 100.0
                + viewport.width.min(viewport.height) * vmin / 100.0
                + viewport.width.max(viewport.height) * vmax / 100.0,
        ),
    }
}
