use super::{MAX_OPEN_TABS, MAX_RECENTLY_CLOSED_TABS};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::windows_app) struct TabId(u64);

impl TabId {
    pub(in crate::windows_app) const fn first() -> Self {
        Self(1)
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
    next_id: u64,
}

impl<T: IdentifiedTab> TabCollection<T> {
    pub(in crate::windows_app) fn new(first: T) -> Self {
        let next_id = first.tab_id().get().saturating_add(1);
        Self {
            tabs: vec![first],
            active: 0,
            next_id,
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
        let id = self.allocate_id();
        self.tabs.push(create(id));
        if activate {
            self.active = self.tabs.len() - 1;
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

    pub(in crate::windows_app) fn activate_relative(&mut self, forward: bool) -> TabId {
        self.active = if forward {
            (self.active + 1) % self.tabs.len()
        } else {
            (self.active + self.tabs.len() - 1) % self.tabs.len()
        };
        self.active_id()
    }

    pub(in crate::windows_app) fn activate_position(&mut self, one_based: usize) -> Option<TabId> {
        let index = one_based.checked_sub(1)?;
        if index >= self.tabs.len() {
            return None;
        }
        self.active = index;
        Some(self.active_id())
    }

    pub(in crate::windows_app) fn activate_last(&mut self) -> TabId {
        self.active = self.tabs.len() - 1;
        self.active_id()
    }

    pub(in crate::windows_app) fn remove_active(&mut self) -> T {
        assert!(self.tabs.len() > 1, "the last tab must be replaced");
        let removed = self.tabs.remove(self.active);
        self.active = self.active.min(self.tabs.len() - 1);
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
        Some(removed)
    }

    pub(in crate::windows_app) fn replace_active(
        &mut self,
        create: impl FnOnce(TabId) -> T,
    ) -> (TabId, T) {
        let id = self.allocate_id();
        let removed = std::mem::replace(&mut self.tabs[self.active], create(id));
        (id, removed)
    }

    fn allocate_id(&mut self) -> TabId {
        let id = TabId(self.next_id);
        self.next_id = self.next_id.saturating_add(1).max(1);
        id
    }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy)]
    struct TestTab(TabId);

    impl IdentifiedTab for TestTab {
        fn tab_id(&self) -> TabId {
            self.0
        }
    }

    #[test]
    fn identities_are_stable_across_close_and_reopen() {
        let mut tabs = TabCollection::new(TestTab(TabId::first()));
        let second = tabs.add(true, TestTab).unwrap();
        let third = tabs.add(true, TestTab).unwrap();
        assert_eq!(tabs.remove_active().tab_id(), third);
        let reopened = tabs.add(true, TestTab).unwrap();

        assert_eq!(second.get(), 2);
        assert_eq!(third.get(), 3);
        assert_eq!(reopened.get(), 4);
    }

    #[test]
    fn close_prefers_the_right_sibling_then_the_previous_tab() {
        let mut tabs = TabCollection::new(TestTab(TabId::first()));
        let second = tabs.add(true, TestTab).unwrap();
        let third = tabs.add(true, TestTab).unwrap();
        tabs.activate(second);
        tabs.remove_active();
        assert_eq!(tabs.active_id(), third);
        tabs.remove_active();
        assert_eq!(tabs.active_id(), TabId::first());
    }

    #[test]
    fn recently_closed_tabs_are_lifo_and_bounded() {
        let mut closed = RecentlyClosedTabs::new();
        for value in 0..=MAX_RECENTLY_CLOSED_TABS {
            closed.push(value);
        }
        assert_eq!(closed.pop(), Some(MAX_RECENTLY_CLOSED_TABS));
        for expected in (1..MAX_RECENTLY_CLOSED_TABS).rev() {
            assert_eq!(closed.pop(), Some(expected));
        }
        assert_eq!(closed.pop(), None);
    }

    #[test]
    fn cycling_and_number_selection_wrap_predictably() {
        let mut tabs = TabCollection::new(TestTab(TabId::first()));
        let second = tabs.add(false, TestTab).unwrap();
        let third = tabs.add(false, TestTab).unwrap();

        assert_eq!(tabs.activate_relative(false), third);
        assert_eq!(tabs.activate_relative(true), TabId::first());
        assert_eq!(tabs.activate_position(2), Some(second));
        assert_eq!(tabs.activate_last(), third);
        assert_eq!(tabs.activate_position(4), None);
    }

    #[test]
    fn closing_a_background_tab_preserves_the_active_identity() {
        let mut tabs = TabCollection::new(TestTab(TabId::first()));
        let second = tabs.add(true, TestTab).unwrap();
        let third = tabs.add(false, TestTab).unwrap();

        assert_eq!(tabs.remove(third).unwrap().tab_id(), third);
        assert_eq!(tabs.active_id(), second);
    }

    #[test]
    fn the_open_tab_bound_is_enforced() {
        let mut tabs = TabCollection::new(TestTab(TabId::first()));
        for _ in 1..MAX_OPEN_TABS {
            tabs.add(false, TestTab).unwrap();
        }
        assert_eq!(tabs.len(), MAX_OPEN_TABS);
        assert_eq!(tabs.add(false, TestTab), Err(TabLimitReached));
    }
}
