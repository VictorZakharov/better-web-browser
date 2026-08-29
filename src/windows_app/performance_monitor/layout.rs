//! Viewport-aware geometry for the diagnostics child window.

use super::*;

const PANEL_WIDTH_DIP: i32 = 480;
const PANEL_HEIGHT_DIP: i32 = 520;
const METRIC_ROW_COUNT: usize = 9;
const GRAPH_MIN_HEIGHT_DIP: i32 = 330;

#[derive(Clone, Copy)]
pub(super) struct PanelLayout {
    pub(super) heading: Rect,
    pub(super) metrics: Rect,
    pub(super) visible_metric_rows: usize,
    pub(super) details: Rect,
    pub(super) graph: Option<Rect>,
    pub(super) copy_button: Rect,
    pub(super) close_button: Rect,
    pub(super) row_height: i32,
    pub(super) scrollbar: Rect,
}

impl PanelLayout {
    pub(super) fn new(client: &Rect, dpi: u32) -> Self {
        let scale = |value| scale_dip(value, dpi);
        let padding = scale(16).max(1);
        let row_height = scale(18).max(1);
        let inner_right = (client.right - padding).max(padding);
        let footer_top = (client.bottom - scale(48)).max(0);

        let close_width = scale(70).max(1);
        let button_gap = scale(8).max(1);
        let button_top = (client.bottom - scale(42)).clamp(0, client.bottom.max(0));
        let button_bottom = (client.bottom - scale(10)).clamp(button_top, client.bottom.max(0));
        let close_left = (inner_right - close_width).max(padding);
        let close_button = Rect {
            left: close_left,
            top: button_top,
            right: inner_right,
            bottom: button_bottom,
        };
        let copy_button = Rect {
            left: padding,
            top: button_top,
            right: (close_left - button_gap).max(padding),
            bottom: button_bottom,
        };

        let heading = Rect {
            left: padding,
            top: scale(8).min(footer_top),
            right: inner_right,
            bottom: scale(38).min(footer_top),
        };
        let metrics_top = scale(40).min(footer_top);
        let graph = if client.height() >= scale(GRAPH_MIN_HEIGHT_DIP) {
            let bottom = (footer_top - scale(10)).max(metrics_top);
            let top = (bottom - scale(32)).max(metrics_top);
            (top < bottom).then_some(Rect {
                left: padding,
                top,
                right: inner_right,
                bottom,
            })
        } else {
            None
        };
        let details_bottom = graph
            .map(|bounds| bounds.top - scale(8))
            .unwrap_or_else(|| footer_top - scale(8))
            .max(metrics_top);
        let details_gap = scale(8).max(1);
        let metric_space = details_bottom - metrics_top - details_gap - row_height;
        let visible_metric_rows =
            (metric_space / row_height).clamp(0, METRIC_ROW_COUNT as i32) as usize;
        let metrics = Rect {
            left: padding,
            top: metrics_top,
            right: inner_right,
            bottom: metrics_top + visible_metric_rows as i32 * row_height,
        };
        let details_top = (metrics.bottom + details_gap).min(details_bottom);
        let scrollbar_width = scale(4).max(2);
        let details = Rect {
            left: padding,
            top: details_top,
            right: inner_right,
            bottom: details_bottom,
        };
        let scrollbar = Rect {
            left: (details.right - scrollbar_width).max(details.left),
            top: details.top,
            right: details.right,
            bottom: details.bottom,
        };
        Self {
            heading,
            metrics,
            visible_metric_rows,
            details,
            graph,
            copy_button,
            close_button,
            row_height,
            scrollbar,
        }
    }

    pub(super) fn visible_detail_rows(&self) -> usize {
        (self.details.height() / self.row_height.max(1)).max(0) as usize
    }

    pub(super) fn detail_text_bounds(&self, scrollable: bool, dpi: u32) -> Rect {
        Rect {
            right: if scrollable {
                (self.scrollbar.left - scale_dip(6, dpi)).max(self.details.left)
            } else {
                self.details.right
            },
            ..self.details
        }
    }

    pub(super) fn scrollbar_thumb(&self, total: usize, offset: usize) -> Option<Rect> {
        let visible = self.visible_detail_rows();
        if visible == 0 || total <= visible || self.scrollbar.height() <= 0 {
            return None;
        }
        let track_height = self.scrollbar.height();
        let thumb_height = ((track_height as i64 * visible as i64 / total as i64) as i32)
            .clamp(self.row_height.min(track_height), track_height);
        let maximum_offset = total - visible;
        let travel = track_height - thumb_height;
        let thumb_top = self.scrollbar.top
            + (travel as i64 * offset.min(maximum_offset) as i64 / maximum_offset as i64) as i32;
        Some(Rect {
            left: self.scrollbar.left,
            top: thumb_top,
            right: self.scrollbar.right,
            bottom: thumb_top + thumb_height,
        })
    }
}

pub(super) fn panel_size(state: &BrowserState) -> Size {
    Size {
        cx: state.scale(PANEL_WIDTH_DIP),
        cy: state.scale(PANEL_HEIGHT_DIP),
    }
}

pub(super) fn panel_layout(state: &BrowserState, client: &Rect) -> PanelLayout {
    PanelLayout::new(client, state.dpi)
}

pub(super) fn clamp_scroll_row(current: usize, total: usize, visible: usize) -> usize {
    current.min(total.saturating_sub(visible))
}

pub(super) fn scroll_detail_rows(
    current: usize,
    wheel_delta: i32,
    total: usize,
    visible: usize,
) -> usize {
    let maximum = total.saturating_sub(visible);
    let current = current.min(maximum);
    match wheel_delta.cmp(&0) {
        std::cmp::Ordering::Less => current.saturating_add(3).min(maximum),
        std::cmp::Ordering::Greater => current.saturating_sub(3),
        std::cmp::Ordering::Equal => current,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(width_dip: i32, height_dip: i32, dpi: u32) -> Rect {
        Rect {
            left: 0,
            top: 0,
            right: scale_dip(width_dip, dpi),
            bottom: scale_dip(height_dip, dpi),
        }
    }

    fn contained(inner: Rect, outer: Rect) -> bool {
        inner.left >= outer.left
            && inner.top >= outer.top
            && inner.right <= outer.right
            && inner.bottom <= outer.bottom
            && inner.left <= inner.right
            && inner.top <= inner.bottom
    }

    #[test]
    fn full_panel_sections_are_contained_at_100_and_125_percent_dpi() {
        for dpi in [96, 120] {
            let client = client(480, 520, dpi);
            let layout = PanelLayout::new(&client, dpi);
            assert_eq!(layout.visible_metric_rows, METRIC_ROW_COUNT);
            assert!(layout.visible_detail_rows() >= 8);
            assert!(contained(layout.heading, client));
            assert!(contained(layout.metrics, client));
            assert!(contained(layout.details, client));
            assert!(contained(layout.graph.unwrap(), client));
            assert!(contained(layout.copy_button, client));
            assert!(contained(layout.close_button, client));
            assert!(layout.details.bottom <= layout.graph.unwrap().top);
            assert!(layout.copy_button.right < layout.close_button.left);
        }
    }

    #[test]
    fn constrained_panel_keeps_controls_and_text_regions_disjoint() {
        let client = client(420, 280, 120);
        let layout = PanelLayout::new(&client, 120);
        assert!(layout.graph.is_none());
        assert!(layout.visible_metric_rows > 0);
        assert!(layout.visible_detail_rows() > 0);
        assert!(layout.metrics.bottom <= layout.details.top);
        assert!(layout.details.bottom <= layout.copy_button.top);
        assert!(contained(layout.copy_button, client));
        assert!(contained(layout.close_button, client));
    }

    #[test]
    fn scrollbar_and_wheel_navigation_stay_bounded() {
        let client = client(480, 430, 96);
        let layout = PanelLayout::new(&client, 96);
        let visible = layout.visible_detail_rows();
        let total = 40;
        let maximum = total - visible;
        assert_eq!(scroll_detail_rows(0, -120, total, visible), 3);
        assert_eq!(scroll_detail_rows(3, 120, total, visible), 0);
        assert_eq!(
            scroll_detail_rows(usize::MAX, -120, total, visible),
            maximum
        );
        let thumb = layout.scrollbar_thumb(total, maximum).unwrap();
        assert!(contained(thumb, layout.scrollbar));
        assert_eq!(thumb.bottom, layout.scrollbar.bottom);
    }
}
