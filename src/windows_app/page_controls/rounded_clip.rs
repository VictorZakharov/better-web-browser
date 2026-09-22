//! Clip native EDIT painting to a rounded CSS backdrop without relying on
//! Win32's unreliable transparent EDIT repaint path.

use super::*;
use better_web_browser::engine::{RectF, css::Color};

fn rounded_backdrop(
    items: &[DisplayItem],
    control: RectF,
    background: Color,
) -> Option<(RectF, f32)> {
    items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::SolidRect {
                rect,
                color,
                radius,
            } if *radius > 0.0
                && color.to_colorref() == background.to_colorref()
                && rect.x <= control.x + 0.5
                && rect.y <= control.y + 0.5
                && rect.right() + 0.5 >= control.right()
                && rect.bottom() + 0.5 >= control.bottom() =>
            {
                Some((*rect, *radius))
            }
            _ => None,
        })
        .min_by(|(a, _), (b, _)| (a.width * a.height).total_cmp(&(b.width * b.height)))
}

impl BrowserState {
    pub(super) unsafe fn clip_transparent_page_controls(&self) {
        let scale = self.page_scale();
        for control in &self.page_controls {
            if control.spec.background_color.alpha != 0
                || !matches!(
                    control.spec.kind,
                    ControlKind::Text
                        | ControlKind::TextArea
                        | ControlKind::Password
                        | ControlKind::Search
                )
            {
                continue;
            }
            let Some((backdrop, radius)) = rounded_backdrop(
                &self.page_layout.items,
                control.spec.rect,
                control.spec.background_color,
            ) else {
                continue;
            };
            let [border_top, _, _, border_left] = control.spec.border_width;
            let [padding_top, _, _, padding_left] = control.spec.padding;
            let native_x = control.spec.rect.x + border_left + padding_left;
            let native_y = control.spec.rect.y + border_top + padding_top;
            // Window regions are relative to the native child, not to the page.
            // The window bounds further intersect this ancestor-shaped region.
            let left = ((backdrop.x - native_x) * scale).round() as i32;
            let top = ((backdrop.y - native_y) * scale).round() as i32;
            let right = ((backdrop.right() - native_x) * scale).ceil() as i32 + 1;
            let bottom = ((backdrop.bottom() - native_y) * scale).ceil() as i32 + 1;
            let diameter = (radius * 2.0 * scale).round().max(1.0) as i32;
            let region = CreateRoundRectRgn(left, top, right, bottom, diameter, diameter);
            if !region.is_null() && SetWindowRgn(control.window, region, 1) == 0 {
                // SetWindowRgn takes ownership only on success.
                DeleteObject(region);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_the_nearest_matching_rounded_backdrop() {
        let items = [
            DisplayItem::SolidRect {
                rect: RectF {
                    x: 0.0,
                    y: 0.0,
                    width: 500.0,
                    height: 100.0,
                },
                color: Color::rgb(51, 51, 51),
                radius: 10.0,
            },
            DisplayItem::SolidRect {
                rect: RectF {
                    x: 12.0,
                    y: 8.0,
                    width: 200.0,
                    height: 40.0,
                },
                color: Color::rgb(51, 51, 51),
                radius: 20.0,
            },
        ];
        let control = RectF {
            x: 12.0,
            y: 8.0,
            width: 180.0,
            height: 40.0,
        };
        assert_eq!(
            rounded_backdrop(&items, control, Color::rgb(51, 51, 51)),
            Some((
                RectF {
                    x: 12.0,
                    y: 8.0,
                    width: 200.0,
                    height: 40.0
                },
                20.0
            ))
        );
        assert_eq!(rounded_backdrop(&items, control, Color::WHITE), None);
    }
}
