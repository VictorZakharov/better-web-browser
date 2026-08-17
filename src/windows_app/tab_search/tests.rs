use super::*;

fn entry(title: &str, url: &str) -> TabSearchEntry {
    TabSearchEntry {
        kind: TabSearchEntryKind::Closed { id: 1 },
        title: title.into(),
        url: url.into(),
    }
}

#[test]
fn search_matches_titles_and_urls_case_insensitively() {
    let wikipedia = entry("Wikipedia", "https://en.wikipedia.org/wiki/Rust");
    assert!(entry_matches(&wikipedia, "wikipedia"));
    assert!(entry_matches(&wikipedia, "rust"));
    assert!(entry_matches(&wikipedia, ""));
    assert!(!entry_matches(&wikipedia, "chromium"));
}
