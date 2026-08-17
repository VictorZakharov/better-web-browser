use super::selection::TabSelection;
use super::{MAX_OPEN_TABS, MAX_RECENTLY_CLOSED_TABS};
use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TAB_ID: AtomicU64 = AtomicU64::new(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::windows_app) struct TabId(u64);

impl TabId {
    pub(in crate::windows_app) const fn first() -> Self {
        Self(1)
    }

    pub(in crate::windows_app) fn allocate() -> Self {
        let id = NEXT_TAB_ID.fetch_add(1, Ordering::Relaxed);
        assert!(id != 0 && id != u64::MAX, "tab identity space exhausted");
        Self(id)
    }

    pub(in crate::windows_app) const fn get(self) -> u64 {
        self.0
    }

    pub(in crate::windows_app) fn from_message(value: usize) -> Option<Self> {
        (value != 0).then_some(Self(value as u64))
    }
}

pub(in crate::windows_app) trait IdentifiedTab {
    fn tab_id(&self) -> TabId;
}

pub(in crate::windows_app) struct TabCollection<T> {
    tabs: Vec<T>,
    active: usize,
    selection: TabSelection,
}

impl<T: IdentifiedTab> TabCollection<T> {
    pub(in crate::windows_app) fn new(first: T) -> Self {
        let first_id = first.tab_id();
        Self {
            tabs: vec![first],
            active: 0,
            selection: TabSelection::new(first_id),
        }
    }

    pub(in crate::windows_app) fn active(&self) -> &T {
        &self.tabs[self.active]
    }

    pub(in crate::windows_app) fn active_mut(&mut self) -> &mut T {
        &mut self.tabs[self.active]
    }

    pub(in crate::windows_app) fn active_id(&self) -> TabId {
        self.active().tab_id()
    }

    pub(in crate::windows_app) fn active_index(&self) -> usize {
        self.active
    }

    pub(in crate::windows_app) fn len(&self) -> usize {
        self.tabs.len()
    }

    pub(in crate::windows_app) fn available_capacity(&self) -> usize {
        MAX_OPEN_TABS.saturating_sub(self.tabs.len())
    }

    pub(in crate::windows_app) fn iter(&self) -> impl ExactSizeIterator<Item = &T> {
        self.tabs.iter()
    }

    pub(in crate::windows_app) fn iter_mut(&mut self) -> impl ExactSizeIterator<Item = &mut T> {
        self.tabs.iter_mut()
    }

    pub(in crate::windows_app) fn get_mut(&mut self, id: TabId) -> Option<&mut T> {
        self.tabs.iter_mut().find(|tab| tab.tab_id() == id)
    }

    pub(in crate::windows_app) fn contains(&self, id: TabId) -> bool {
        self.tabs.iter().any(|tab| tab.tab_id() == id)
    }

    pub(in crate::windows_app) fn add(
        &mut self,
        activate: bool,
        create: impl FnOnce(TabId) -> T,
    ) -> Result<TabId, TabLimitReached> {
        if self.tabs.len() >= MAX_OPEN_TABS {
            return Err(TabLimitReached);
        }
        let id = TabId::allocate();
        self.tabs.push(create(id));
        if activate {
            self.active = self.tabs.len() - 1;
            self.selection.select_only(id);
        }
        Ok(id)
    }

    pub(in crate::windows_app) fn activate(&mut self, id: TabId) -> bool {
        let Some(index) = self.tabs.iter().position(|tab| tab.tab_id() == id) else {
            return false;
        };
        let changed = self.active != index;
        self.active = index;
        changed
    }

    pub(in crate::windows_app) fn activate_exclusive(&mut self, id: TabId) -> bool {
        let changed = self.activate(id);
        if self.contains(id) {
            self.selection.select_only(id);
        }
        changed
    }

    pub(in crate::windows_app) fn toggle_selection(&mut self, id: TabId) -> bool {
        let previous = self.active_id();
        if !self.contains(id) {
            return false;
        }
        self.selection.toggle(id);
        if self.selection.is_selected(id) {
            self.activate(id);
        } else if previous == id
            && let Some(next) = self.selected_ids().first().copied()
        {
            self.activate(next);
        }
        self.active_id() != previous
    }

    pub(in crate::windows_app) fn select_range(&mut self, id: TabId) -> bool {
        let previous = self.active_id();
        let order = self
            .tabs
            .iter()
            .map(IdentifiedTab::tab_id)
            .collect::<Vec<_>>();
        self.selection.select_range(&order, id);
        self.activate(id);
        self.active_id() != previous
    }

    pub(in crate::windows_app) fn is_selected(&self, id: TabId) -> bool {
        self.selection.is_selected(id)
    }

    pub(in crate::windows_app) fn selection_len(&self) -> usize {
        self.selection.len()
    }

    pub(in crate::windows_app) fn selected_ids(&self) -> Vec<TabId> {
        self.tabs
            .iter()
            .map(IdentifiedTab::tab_id)
            .filter(|id| self.selection.is_selected(*id))
            .collect()
    }

    pub(in crate::windows_app) fn activate_relative(&mut self, forward: bool) -> TabId {
        self.active = if forward {
            (self.active + 1) % self.tabs.len()
        } else {
            (self.active + self.tabs.len() - 1) % self.tabs.len()
        };
        let id = self.active_id();
        self.selection.select_only(id);
        id
    }

    pub(in crate::windows_app) fn activate_position(&mut self, one_based: usize) -> Option<TabId> {
        let index = one_based.checked_sub(1)?;
        if index >= self.tabs.len() {
            return None;
        }
        self.active = index;
        let id = self.active_id();
        self.selection.select_only(id);
        Some(id)
    }

    pub(in crate::windows_app) fn activate_last(&mut self) -> TabId {
        self.active = self.tabs.len() - 1;
        let id = self.active_id();
        self.selection.select_only(id);
        id
    }

    pub(in crate::windows_app) fn remove_active(&mut self) -> T {
        assert!(self.tabs.len() > 1, "the last tab must be replaced");
        let removed = self.tabs.remove(self.active);
        self.active = self.active.min(self.tabs.len() - 1);
        self.retain_valid_selection();
        removed
    }

    pub(in crate::windows_app) fn remove(&mut self, id: TabId) -> Option<T> {
        if self.tabs.len() <= 1 {
            return None;
        }
        let index = self.tabs.iter().position(|tab| tab.tab_id() == id)?;
        let removed = self.tabs.remove(index);
        if index < self.active {
            self.active -= 1;
        } else if index == self.active {
            self.active = self.active.min(self.tabs.len() - 1);
        }
        self.retain_valid_selection();
        Some(removed)
    }

    pub(in crate::windows_app) fn replace_active(
        &mut self,
        create: impl FnOnce(TabId) -> T,
    ) -> (TabId, T) {
        let id = TabId::allocate();
        let removed = std::mem::replace(&mut self.tabs[self.active], create(id));
        self.selection.select_only(id);
        (id, removed)
    }

    pub(in crate::windows_app) fn reorder_selected(&mut self, target_index: usize) {
        let selected = self.selected_ids().into_iter().collect::<HashSet<_>>();
        if selected.is_empty() {
            return;
        }
        let active = self.active_id();
        let mut moved = Vec::with_capacity(selected.len());
        let mut retained = Vec::with_capacity(self.tabs.len() - selected.len());
        let mut removed_before = 0;
        for (index, tab) in self.tabs.drain(..).enumerate() {
            if selected.contains(&tab.tab_id()) {
                if index < target_index {
                    removed_before += 1;
                }
                moved.push(tab);
            } else {
                retained.push(tab);
            }
        }
        let insertion = target_index
            .saturating_sub(removed_before)
            .min(retained.len());
        retained.splice(insertion..insertion, moved);
        self.tabs = retained;
        self.active = self
            .tabs
            .iter()
            .position(|tab| tab.tab_id() == active)
            .unwrap_or(0);
    }

    pub(in crate::windows_app) fn move_selected(&mut self, forward: bool) -> bool {
        let selected = self.selected_ids().into_iter().collect::<HashSet<_>>();
        let selected_indices = self
            .tabs
            .iter()
            .enumerate()
            .filter(|(_, tab)| selected.contains(&tab.tab_id()))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let Some(first) = selected_indices.first().copied() else {
            return false;
        };
        let last = *selected_indices.last().unwrap();
        let target = if forward {
            if last + 1 >= self.tabs.len() {
                return false;
            }
            last + 2
        } else {
            if first == 0 {
                return false;
            }
            first - 1
        };
        self.reorder_selected(target);
        true
    }

    pub(in crate::windows_app) fn extract_selected(&mut self, fallback: T) -> TabBatch<T> {
        let selected = self.selected_ids().into_iter().collect::<HashSet<_>>();
        let active = self.active_id();
        let mut moved = Vec::with_capacity(selected.len());
        let mut retained = Vec::with_capacity(self.tabs.len().saturating_sub(selected.len()));
        for tab in self.tabs.drain(..) {
            if selected.contains(&tab.tab_id()) {
                moved.push(tab);
            } else {
                retained.push(tab);
            }
        }
        if retained.is_empty() {
            retained.push(fallback);
        }
        self.tabs = retained;
        self.active = self.active.min(self.tabs.len() - 1);
        self.retain_valid_selection();
        TabBatch {
            tabs: moved,
            active,
            selected,
        }
    }

    pub(in crate::windows_app) fn insert_batch(&mut self, index: usize, batch: TabBatch<T>) {
        let insertion = index.min(self.tabs.len());
        self.tabs.splice(insertion..insertion, batch.tabs);
        self.active = self
            .tabs
            .iter()
            .position(|tab| tab.tab_id() == batch.active)
            .unwrap_or(insertion.min(self.tabs.len() - 1));
        self.selection.replace(batch.selected, batch.active);
    }

    fn retain_valid_selection(&mut self) {
        let valid = self
            .tabs
            .iter()
            .map(IdentifiedTab::tab_id)
            .collect::<HashSet<_>>();
        self.selection.retain(&valid, self.active_id());
    }
}

pub(in crate::windows_app) struct TabBatch<T> {
    pub(in crate::windows_app) tabs: Vec<T>,
    pub(in crate::windows_app) active: TabId,
    pub(in crate::windows_app) selected: HashSet<TabId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::windows_app) struct TabLimitReached;

pub(in crate::windows_app) struct RecentlyClosedTabs<T> {
    entries: VecDeque<T>,
}

impl<T> RecentlyClosedTabs<T> {
    pub(in crate::windows_app) fn new() -> Self {
        Self {
            entries: VecDeque::new(),
        }
    }

    pub(in crate::windows_app) fn push(&mut self, entry: T) {
        if self.entries.len() == MAX_RECENTLY_CLOSED_TABS {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    pub(in crate::windows_app) fn pop(&mut self) -> Option<T> {
        self.entries.pop_back()
    }

    pub(in crate::windows_app) fn iter_newest(&self) -> impl Iterator<Item = &T> {
        self.entries.iter().rev()
    }

    pub(in crate::windows_app) fn remove_where(
        &mut self,
        predicate: impl FnMut(&T) -> bool,
    ) -> Option<T> {
        let index = self.entries.iter().position(predicate)?;
        self.entries.remove(index)
    }
}

#[cfg(test)]
#[path = "collection/tests.rs"]
mod tests;
