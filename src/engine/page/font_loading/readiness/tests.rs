use super::*;

fn page(source: &str) -> Page {
    let mut page = Page::parse(
        &format!(
            "<style>@font-face{{font-family:Remote;src:{source}}}p{{font-family:Remote}}</style><p>A</p>"
        ),
        "https://example.test/",
    );
    page.refresh_resources_for_viewport(800.0, 600.0);
    page
}

fn fonts(page: &Page) -> Vec<PageResource> {
    page.resources
        .iter()
        .filter(|resource| matches!(resource, PageResource::Font { .. }))
        .cloned()
        .collect()
}

#[test]
fn fallback_candidates_keep_one_original_source_pending_until_terminal_completion() {
    let mut page = page("url(first.ttf),url(second.ttf),url(last.ttf)");
    let mut completed = HashSet::new();
    let wanted = page.pending_css_font_sources(&completed);
    assert_eq!(wanted.len(), 1);
    assert_eq!(page.pending_css_font_sources(&completed), wanted);
    for index in 0..2 {
        let current = fonts(&page)[index].clone();
        completed.insert(current.clone());
        assert!(page.retry_font_resource(&current));
        assert_eq!(page.pending_css_font_sources(&completed), wanted);
    }
    completed.insert(fonts(&page)[2].clone());
    assert!(page.pending_css_font_sources(&completed).is_empty());
}

#[test]
fn unused_rules_do_not_block_readiness_or_trigger_font_downloads() {
    let mut page = Page::parse(
        "<style>@font-face{font-family:Unused;src:url(unused.ttf)}</style><p>System font</p>",
        "https://example.test/",
    );
    page.refresh_resources_for_viewport(800.0, 600.0);
    assert!(fonts(&page).is_empty());
    assert!(page.pending_css_font_sources(&HashSet::new()).is_empty());
}

#[test]
fn removing_or_changing_the_owning_rule_does_not_leave_readiness_stuck() {
    for replacement in ["", "@font-face{font-family:Remote;src:url(other.ttf)}"] {
        let page = page("url(first.ttf),url(second.ttf)");
        assert!(!page.pending_css_font_sources(&HashSet::new()).is_empty());
        let sheet = page.dom.elements_named("style").next().unwrap();
        crate::engine::dom::Node::set_text_content(&sheet, replacement);
        assert!(page.pending_css_font_sources(&HashSet::new()).is_empty());
    }
}

#[test]
fn decoded_fallback_is_loaded_under_the_original_source_identity() {
    let mut page = page("url(first.ttf),url(second.ttf)");
    let original = fonts(&page)[0].clone();
    assert!(page.retry_font_resource(&original));
    page.add_font(
        "https://example.test/first.ttf".into(),
        "Remote".into(),
        400,
        false,
        include_bytes!("../../../../../tests/canvas/fonts/ahem.ttf"),
    )
    .unwrap();
    assert!(
        page.pending_css_font_sources(&HashSet::from([original]))
            .is_empty()
    );
}
