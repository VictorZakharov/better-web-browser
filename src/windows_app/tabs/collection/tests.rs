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

    assert_ne!(second, third);
    assert_ne!(third, reopened);
    assert_ne!(second, reopened);
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

#[test]
fn ctrl_and_shift_selection_preserve_order() {
    let mut tabs = TabCollection::new(TestTab(TabId::first()));
    let second = tabs.add(false, TestTab).unwrap();
    let third = tabs.add(false, TestTab).unwrap();
    let fourth = tabs.add(false, TestTab).unwrap();

    tabs.toggle_selection(second);
    tabs.select_range(fourth);

    assert_eq!(tabs.selected_ids(), [second, third, fourth]);
    assert_eq!(tabs.active_id(), fourth);
}

#[test]
fn selected_tabs_reorder_and_transfer_as_one_ordered_batch() {
    let mut source = TabCollection::new(TestTab(TabId::first()));
    let second = source.add(false, TestTab).unwrap();
    let third = source.add(false, TestTab).unwrap();
    let fourth = source.add(false, TestTab).unwrap();
    source.activate_exclusive(second);
    source.toggle_selection(fourth);

    source.reorder_selected(4);
    assert_eq!(
        source.iter().map(IdentifiedTab::tab_id).collect::<Vec<_>>(),
        [TabId::first(), third, second, fourth]
    );

    let placeholder = source.add(false, TestTab).unwrap();
    let batch = source.extract_selected(TestTab(TabId::allocate()));
    let mut target = TabCollection::new(TestTab(TabId::allocate()));
    target.insert_batch(1, batch);
    assert_eq!(target.selected_ids(), [second, fourth]);
    assert_eq!(target.active_id(), fourth);
    assert!(source.contains(placeholder));
}

#[test]
fn transferring_every_selected_tab_installs_the_required_source_fallback() {
    let mut source = TabCollection::new(TestTab(TabId::first()));
    let second = source.add(false, TestTab).unwrap();
    source.select_range(second);
    let fallback = TabId::allocate();

    let batch = source.extract_selected(TestTab(fallback));

    assert_eq!(batch.tabs.len(), 2);
    assert_eq!(source.len(), 1);
    assert_eq!(source.active_id(), fallback);
    assert_eq!(source.selected_ids(), [fallback]);
}

#[test]
fn selected_tabs_move_left_and_right_as_one_block() {
    let mut tabs = TabCollection::new(TestTab(TabId::first()));
    let second = tabs.add(false, TestTab).unwrap();
    let third = tabs.add(false, TestTab).unwrap();
    let fourth = tabs.add(false, TestTab).unwrap();
    tabs.activate_exclusive(second);
    tabs.toggle_selection(third);

    assert!(tabs.move_selected(true));
    assert_eq!(
        tabs.iter().map(IdentifiedTab::tab_id).collect::<Vec<_>>(),
        [TabId::first(), fourth, second, third]
    );
    assert!(tabs.move_selected(false));
    assert_eq!(
        tabs.iter().map(IdentifiedTab::tab_id).collect::<Vec<_>>(),
        [TabId::first(), second, third, fourth]
    );
}

#[test]
fn a_specific_recently_closed_entry_can_be_removed_without_disturbing_newer_entries() {
    let mut closed = RecentlyClosedTabs::new();
    closed.push("first");
    closed.push("second");
    closed.push("third");

    assert_eq!(
        closed.remove_where(|entry| *entry == "second"),
        Some("second")
    );
    assert_eq!(closed.pop(), Some("third"));
    assert_eq!(closed.pop(), Some("first"));
}
