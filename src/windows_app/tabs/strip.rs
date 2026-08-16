use super::TabId;
use crate::windows_app::{Rect, scale_dip};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::windows_app) enum TabStripHit {
    Activate(TabId),
    Close(TabId),
    NewTab,
}

#[derive(Clone, Copy)]
pub(in crate::windows_app) struct TabRegion {
    pub(in crate::windows_app) id: TabId,
    pub(in crate::windows_app) bounds: Rect,
    pub(in crate::windows_app) close: Option<Rect>,
}

pub(in crate::windows_app) struct TabStripLayout {
    pub(in crate::windows_app) tabs: Vec<TabRegion>,
    pub(in crate::windows_app) new_tab: Rect,
}

impl TabStripLayout {
    pub(in crate::windows_app) fn calculate(client_width: i32, dpi: u32, ids: &[TabId]) -> Self {
        let scale = |value| scale_dip(value, dpi);
        let margin = scale(10);
        let gap = scale(3);
        let new_width = scale(38);
        let top = scale(5);
        let bottom = scale(37);
        let available = (client_width - margin * 2 - new_width - gap).max(ids.len() as i32);
        let tab_width = if ids.is_empty() {
            scale(160)
        } else {
            (available / ids.len() as i32).min(scale(220)).max(1)
        };
        let mut left = margin;
        let tabs = ids
            .iter()
            .copied()
            .map(|id| {
                let bounds = Rect {
                    left,
                    top,
                    right: (left + tab_width).min(client_width - margin - new_width - gap),
                    bottom,
                };
                left = bounds.right;
                let close = (bounds.width() >= scale(62)).then_some(Rect {
                    left: bounds.right - scale(30),
                    top: bounds.top + scale(4),
                    right: bounds.right - scale(5),
                    bottom: bounds.bottom - scale(4),
                });
                TabRegion { id, bounds, close }
            })
            .collect();
        let new_tab = Rect {
            left: (left + gap).min(client_width - margin - new_width),
            top,
            right: (left + gap + new_width).min(client_width - margin),
            bottom,
        };
        Self { tabs, new_tab }
    }

    pub(in crate::windows_app) fn hit_test(&self, x: i32, y: i32) -> Option<TabStripHit> {
        if contains(&self.new_tab, x, y) {
            return Some(TabStripHit::NewTab);
        }
        self.tabs.iter().find_map(|tab| {
            if tab.close.is_some_and(|close| contains(&close, x, y)) {
                Some(TabStripHit::Close(tab.id))
            } else if contains(&tab.bounds, x, y) {
                Some(TabStripHit::Activate(tab.id))
            } else {
                None
            }
        })
    }
}

fn contains(rect: &Rect, x: i32, y: i32) -> bool {
    x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_hit_takes_precedence_over_tab_activation() {
        let layout = TabStripLayout::calculate(900, 96, &[TabId::first()]);
        let close = layout.tabs[0].close.unwrap();
        assert_eq!(
            layout.hit_test(close.left + 1, close.top + 1),
            Some(TabStripHit::Close(TabId::first()))
        );
    }
}
