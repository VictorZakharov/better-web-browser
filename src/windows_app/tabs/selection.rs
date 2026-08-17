use super::TabId;
use std::collections::HashSet;

pub(in crate::windows_app) struct TabSelection {
    selected: HashSet<TabId>,
    anchor: TabId,
}

impl TabSelection {
    pub(in crate::windows_app) fn new(first: TabId) -> Self {
        Self {
            selected: HashSet::from([first]),
            anchor: first,
        }
    }

    pub(in crate::windows_app) fn is_selected(&self, id: TabId) -> bool {
        self.selected.contains(&id)
    }

    pub(in crate::windows_app) fn len(&self) -> usize {
        self.selected.len()
    }

    pub(in crate::windows_app) fn select_only(&mut self, id: TabId) {
        self.selected.clear();
        self.selected.insert(id);
        self.anchor = id;
    }

    pub(in crate::windows_app) fn toggle(&mut self, id: TabId) {
        if self.selected.contains(&id) && self.selected.len() > 1 {
            self.selected.remove(&id);
        } else {
            self.selected.insert(id);
        }
        self.anchor = id;
    }

    pub(in crate::windows_app) fn select_range(&mut self, order: &[TabId], id: TabId) {
        let Some(anchor) = order.iter().position(|candidate| *candidate == self.anchor) else {
            self.select_only(id);
            return;
        };
        let Some(target) = order.iter().position(|candidate| *candidate == id) else {
            return;
        };
        let (start, end) = if anchor <= target {
            (anchor, target)
        } else {
            (target, anchor)
        };
        self.selected.clear();
        self.selected.extend(order[start..=end].iter().copied());
    }

    pub(in crate::windows_app) fn replace(
        &mut self,
        ids: impl IntoIterator<Item = TabId>,
        anchor: TabId,
    ) {
        self.selected.clear();
        self.selected.extend(ids);
        self.anchor = anchor;
    }

    pub(in crate::windows_app) fn retain(&mut self, valid: &HashSet<TabId>, fallback: TabId) {
        self.selected.retain(|id| valid.contains(id));
        if self.selected.is_empty() {
            self.selected.insert(fallback);
        }
        if !valid.contains(&self.anchor) {
            self.anchor = fallback;
        }
    }
}
