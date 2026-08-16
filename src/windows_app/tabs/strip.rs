use super::TabId;
use crate::windows_app::{Rect, scale_dip};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::windows_app) enum TabStripHit {
    Activate(TabId),
    Close(TabId),
    NewTab,
    SearchTabs,
}

#[derive(Clone, Copy)]
pub(in crate::windows_app) struct TabRegion {
    pub(in crate::windows_app) id: TabId,
    pub(in crate::windows_app) bounds: Rect,
    pub(in crate::windows_app) close: Option<Rect>,
}

pub(in crate::windows_app) struct TabStripLayout {
    pub(in crate::windows_app) search_tabs: Rect,
    pub(in crate::windows_app) tabs: Vec<TabRegion>,
    pub(in crate::windows_app) new_tab: Rect,
}

impl TabStripLayout {
    pub(in crate::windows_app) fn calculate(client_width: i32, dpi: u32, ids: &[TabId]) -> Self {
        let scale = |value| scale_dip(value, dpi);
        let margin = scale(10);
        let gap = scale(3);
        let search_width = scale(36);
        let new_width = scale(38);
        let top = scale(5);
        let bottom = scale(40);
        let search_tabs = Rect {
            left: scale(4),
            top,
            right: scale(4) + search_width,
            bottom,
        };
        let tab_start = search_tabs.right + gap;
        let available = (client_width - tab_start - margin - new_width - gap).max(ids.len() as i32);
        let tab_width = if ids.is_empty() {
            scale(160)
        } else {
            (available / ids.len() as i32).min(scale(220)).max(1)
        };
        let mut left = tab_start;
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
        Self {
            search_tabs,
            tabs,
            new_tab,
        }
    }

    pub(in crate::windows_app) fn hit_test(&self, x: i32, y: i32) -> Option<TabStripHit> {
        if contains(&self.search_tabs, x, y) {
            return Some(TabStripHit::SearchTabs);
        }
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

    pub(in crate::windows_app) fn insertion_index(&self, x: i32) -> usize {
        self.tabs
            .iter()
            .position(|tab| x < tab.bounds.left + tab.bounds.width() / 2)
            .unwrap_or(self.tabs.len())
    }

    pub(in crate::windows_app) fn contains_strip_y(&self, y: i32) -> bool {
        y >= self.search_tabs.top && y < self.search_tabs.bottom
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

    #[test]
    fn search_control_is_reserved_before_tabs_and_drop_indices_follow_midpoints() {
        let ids = [TabId::first(), TabId::allocate(), TabId::allocate()];
        let layout = TabStripLayout::calculate(900, 96, &ids);
        assert!(layout.search_tabs.right <= layout.tabs[0].bounds.left);
        assert_eq!(
            layout.hit_test(layout.search_tabs.left + 1, layout.search_tabs.top + 1),
            Some(TabStripHit::SearchTabs)
        );
        assert_eq!(layout.insertion_index(layout.tabs[0].bounds.left), 0);
        assert_eq!(layout.insertion_index(layout.tabs[2].bounds.right), 3);
        assert_eq!(layout.tabs[0].bounds.bottom, scale_dip(40, 96));
    }
}
