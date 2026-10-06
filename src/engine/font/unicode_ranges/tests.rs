use super::*;

#[test]
fn existing_css_parser_handles_single_ranges_wildcards_and_serialization() {
    let ranges = UnicodeRanges::parse("u+0041, U+4??, U+1f600-1f64f").unwrap();
    assert_eq!(ranges.serialize(), "U+41, U+400-4FF, U+1F600-1F64F");
    for point in [0x41, 0x400, 0x4ff, 0x1f600, 0x1f64f] {
        assert!(ranges.contains(point));
    }
    for point in [0x40, 0x42, 0x3ff, 0x500, 0x1f5ff, 0x1f650] {
        assert!(!ranges.contains(point));
    }
}

#[test]
fn malformed_or_unbounded_descriptors_are_not_successful_coverage() {
    for source in [
        "",
        "U+",
        "U+110000",
        "U+FFFF-0000",
        "U+1234567",
        "U+41,",
        "41",
        "U+41;",
        "U+41 garbage",
        "U+?F",
        "U+4?????",
        "U+41,,U+42",
        "U + 41",
    ] {
        assert!(UnicodeRanges::parse(source).is_none(), "{source}");
    }
    assert!(UnicodeRanges::parse(&" ".repeat(MAX_SOURCE_BYTES + 1)).is_none());
    assert!(UnicodeRanges::parse(&vec!["U+41"; MAX_RANGES + 1].join(",")).is_none());
    assert!(UnicodeRanges::parse(&vec!["U+41"; MAX_RANGES].join(",")).is_some());
}

#[test]
fn supplementary_characters_are_one_codepoint_and_empty_text_does_not_intersect() {
    let emoji = UnicodeRanges::parse("U+1F600").unwrap();
    assert!(emoji.intersects("a😀b"));
    assert!(!emoji.intersects("a😃b"));
    assert!(!emoji.intersects(""));
    assert!(UnicodeRanges::default().contains(0x10ffff));
    assert!(!UnicodeRanges::default().contains(0x110000));
}

#[test]
fn snapshots_share_only_immutable_ranges() {
    let original = UnicodeRanges::parse("U+41-5A").unwrap();
    let copy = original.clone();
    assert!(Arc::ptr_eq(&original.0, &copy.0));
    assert_eq!(original, copy);
    assert_ne!(original, UnicodeRanges::parse("U+61-7A").unwrap());
}
