use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn effective_background_color(&self, node: &NodeRef) -> Color {
        let mut colors = Vec::new();
        let mut candidate = Some(node.clone());
        while let Some(current) = candidate {
            let color = self.styles.get(&current).background_color;
            if color.alpha > 0 {
                colors.push(color);
            }
            candidate = Node::composed_parent(&current);
        }
        colors
            .into_iter()
            .rev()
            .fold(Color::WHITE, |backdrop, color| {
                color.composite_over(backdrop)
            })
    }

    pub(super) fn background_tile_rect(
        &self,
        style: &ComputedStyle,
        clip_rect: RectF,
    ) -> Option<RectF> {
        let url = style.background_image.as_ref()?;
        let image = self.page.images.get(url)?;
        let (width, height) = resolve_background_size(
            style.background_size,
            clip_rect,
            image.width as f32,
            image.height as f32,
            style.font_size,
            self.viewport,
        )?;
        let x = clip_rect.x
            + resolve_background_position(
                style.background_position_x,
                clip_rect.width,
                width,
                style.font_size,
                self.viewport,
            );
        let y = clip_rect.y
            + resolve_background_position(
                style.background_position_y,
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
        Some((style.background_image.clone()?, tile.width, tile.height))
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
