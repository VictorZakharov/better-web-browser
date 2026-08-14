use super::*;

pub(super) fn translate_display_items(items: &mut [DisplayItem], offset_x: f32, offset_y: f32) {
    for item in items {
        let rect = match item {
            DisplayItem::SolidRect { rect, .. }
            | DisplayItem::BorderRect { rect, .. }
            | DisplayItem::Text { rect, .. }
            | DisplayItem::Image { rect, .. } => rect,
            DisplayItem::BackgroundImage {
                clip_rect,
                tile_rect,
                ..
            } => {
                clip_rect.x += offset_x;
                clip_rect.y += offset_y;
                tile_rect.x += offset_x;
                tile_rect.y += offset_y;
                continue;
            }
            DisplayItem::Control(spec) => &mut spec.rect,
        };
        rect.x += offset_x;
        rect.y += offset_y;
    }
}
