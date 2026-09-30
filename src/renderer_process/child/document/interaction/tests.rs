use super::*;

#[test]
fn hit_target_cursor_distinguishes_links_from_ordinary_content() {
    assert_eq!(cursor_for_link(true), PointerCursor::Pointer);
    assert_eq!(cursor_for_link(false), PointerCursor::Default);
}

#[test]
fn keyboard_compatibility_codes_match_native_windows_input() {
    assert_eq!(key_code("k"), 75);
    assert_eq!(key_code("ArrowRight"), 39);
    assert_eq!(key_code("Escape"), 27);
}
