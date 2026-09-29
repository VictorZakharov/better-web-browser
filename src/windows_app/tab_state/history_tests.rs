use super::*;

#[test]
fn same_document_entries_inherit_restoration_mode_and_saved_viewport_position() {
    use better_web_browser::renderer_protocol::ScrollRestorationMode;

    let mut tab = BrowserTab::new(TabId::first());
    assert_eq!(
        tab.history[0].scroll_restoration,
        ScrollRestorationMode::Auto
    );
    tab.history[0].scroll_restoration = ScrollRestorationMode::Manual;
    tab.history[0].scroll_y = Some(237.6);
    let next = HistoryEntry::same_document(
        "https://example.test/#two".into(),
        &tab.history[0],
        DocumentId::new(7).unwrap(),
        None,
    );
    assert_eq!(next.scroll_restoration, ScrollRestorationMode::Manual);
    assert_eq!(next.scroll_y, Some(237.6));
    tab.push_history_entry(next);
    assert_eq!(tab.history[0].scroll_y, Some(237.6));
    assert_eq!(
        tab.history[1].scroll_restoration,
        ScrollRestorationMode::Manual
    );
    assert_eq!(
        HistoryEntry::new("https://example.test/other".into()).scroll_restoration,
        ScrollRestorationMode::Auto
    );
}
use better_web_browser::limits::MAX_SESSION_HISTORY_ENTRIES;

#[test]
fn replacement_of_initial_home_is_one_entry_not_a_synthetic_back_destination() {
    let mut tab = BrowserTab::new(TabId::first());
    tab.replace_current_history_url("https://example.test/start".into());

    assert_eq!(tab.history.len(), 1);
    assert_eq!(tab.history_index, 0);
    assert_eq!(tab.current_url(), Some("https://example.test/start"));
    assert!(tab.history[0].document.is_none());
    assert!(tab.history[0].state.is_none());
}

#[test]
fn pushing_after_back_truncates_the_forward_branch() {
    let mut tab = BrowserTab::new(TabId::first());
    tab.push_history_entry(HistoryEntry::new("https://example.test/one".into()));
    tab.push_history_entry(HistoryEntry::new("https://example.test/two".into()));
    tab.history_index = 1;

    tab.push_history_entry(HistoryEntry::new("https://example.test/three".into()));

    assert_eq!(tab.history.len(), 3);
    assert_eq!(tab.history_index, 2);
    assert_eq!(tab.current_url(), Some("https://example.test/three"));
    assert_eq!(tab.history[1].url, "https://example.test/one");
    assert_eq!(tab.history[2].url, "https://example.test/three");
}

#[test]
fn history_eviction_keeps_the_active_entry_and_bounded_length() {
    let mut tab = BrowserTab::new(TabId::first());
    for index in 0..MAX_SESSION_HISTORY_ENTRIES {
        tab.push_history_entry(HistoryEntry::new(format!("https://example.test/{index}")));
    }

    assert_eq!(tab.history.len(), MAX_SESSION_HISTORY_ENTRIES);
    assert_eq!(tab.history_index, MAX_SESSION_HISTORY_ENTRIES - 1);
    assert_eq!(tab.history[0].url, "https://example.test/0");
    assert_eq!(
        tab.current_url(),
        Some(format!("https://example.test/{}", MAX_SESSION_HISTORY_ENTRIES - 1).as_str())
    );
}

#[test]
fn fallback_rebinds_all_entries_sharing_a_document_state() {
    let mut tab = BrowserTab::new(TabId::first());
    tab.replace_current_history_url("https://example.test/start".into());
    let original = DocumentId::new(11).unwrap();
    tab.commit_history_document("https://example.test/start", false, original);
    let first = HistoryEntry::same_document(
        "https://example.test/one".into(),
        &tab.history[0],
        original,
        Some("first state".into()),
    );
    tab.push_history_entry(first);
    let second = HistoryEntry::same_document(
        "https://example.test/two".into(),
        &tab.history[1],
        original,
        Some("second state".into()),
    );
    tab.push_history_entry(second);
    tab.push_history_entry(HistoryEntry::new("https://example.test/other".into()));

    tab.retire_history_documents();
    tab.history_index = 2;
    let restored = DocumentId::new(12).unwrap();
    tab.commit_history_document("https://example.test/two", false, restored);

    assert!(
        tab.history[..3]
            .iter()
            .all(|entry| entry.document == Some(restored))
    );
    assert_eq!(tab.history[3].document, None);
    assert_eq!(tab.history[1].state.as_deref(), Some("first state"));
    assert_eq!(tab.history[2].state.as_deref(), Some("second state"));
}

#[test]
fn redirect_detaches_only_the_repopulated_entry_from_its_old_group() {
    let mut tab = BrowserTab::new(TabId::first());
    tab.replace_current_history_url("https://example.test/start".into());
    let first = DocumentId::new(21).unwrap();
    tab.commit_history_document("https://example.test/start", false, first);
    tab.push_history_entry(HistoryEntry::same_document(
        "https://example.test/route".into(),
        &tab.history[0],
        first,
        Some("private state".into()),
    ));
    let old_group = tab.history[0].group;

    tab.retire_history_documents();
    let redirected = DocumentId::new(22).unwrap();
    tab.commit_history_document("https://example.test/route", true, redirected);

    assert_ne!(tab.history[1].group, old_group);
    assert_eq!(tab.history[1].document, Some(redirected));
    assert_eq!(tab.history[1].state, None);
    assert_eq!(tab.history[0].document, None);
}

#[test]
fn closed_tab_keeps_logical_groups_but_no_live_renderer_identity() {
    let mut tab = BrowserTab::new(TabId::first());
    let document = DocumentId::new(31).unwrap();
    tab.commit_history_document(HOME_URL, false, document);
    tab.push_history_entry(HistoryEntry::same_document(
        format!("{HOME_URL}#saved"),
        &tab.history[0],
        document,
        None,
    ));

    let closed = ClosedTab::from(&tab);
    assert_eq!(closed.history[0].group, closed.history[1].group);
    assert!(closed.history.iter().all(|entry| entry.document.is_none()));
}
