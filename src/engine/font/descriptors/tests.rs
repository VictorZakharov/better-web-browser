use super::*;

const BASE: &str = "https://example.test/css/style.css";

#[test]
fn declaration_tokens_preserve_strings_comments_escapes_and_last_valid_value() {
    let face = parse(
        r#"
        /* ; font-family: Bad; */ font-family: First;
        font-family: "Some; Family";
        font-family: serif; font-family: inherit;
        src: url(old); src: url("../new;file.ttf"); src: bogus;
        font-weight: 300 /* between numbers */ 530.5; font-weight: bolder;
        font-style: ITALIC; font-style: unknown;
        unicode-range: U+41-5A; unicode-range: invalid;
    "#,
        BASE,
    )
    .unwrap();
    assert_eq!(face.family, "Some; Family");
    assert_eq!(face.url, "https://example.test/new;file.ttf");
    assert_eq!((face.weight_min, face.weight_max), (300.0, 530.5));
    assert!(face.italic);
    assert_eq!(face.unicode_range, "U+41-5A");
    let escaped = parse(r"font-f\61mily: F\6fo; s\72 c: u\72l(new);", BASE).unwrap();
    assert_eq!(escaped.family, "Foo");
    assert_eq!(escaped.url, "https://example.test/css/new");
}

#[test]
fn important_is_not_accepted_in_descriptors_or_hidden_inside_a_fallback_list() {
    let face = parse(
        "font-family: Good; src:url(good); font-family:Bad!important; \
        src:url(bad),url(worse)!important; font-weight:700!important",
        BASE,
    )
    .unwrap();
    assert_eq!(face.family, "Good");
    assert_eq!(face.url, "https://example.test/css/good");
    assert_eq!(face.weight, 400);
    assert_eq!(parse("font-family: A!important;src:url(a)", BASE), None);
    assert_eq!(parse("font-family: A;src:url(a)!important", BASE), None);
    assert_eq!(
        parse("font-family:'!important';src:url('!important.ttf')", BASE)
            .unwrap()
            .family,
        "!important"
    );
}

#[test]
fn missing_or_invalid_required_descriptors_do_not_install_an_anonymous_face() {
    for body in [
        "src:url(a)",
        "font-family:A",
        "font-family:A,B;src:url(a)",
        "font-family:serif;src:url(a)",
        "font-family:A;src:url(a.ttf)format(svg)",
    ] {
        assert!(parse(body, BASE).is_none(), "{body}");
    }
    assert_eq!(
        parse("font-family:'serif';src:url(font)", BASE)
            .unwrap()
            .family,
        "serif"
    );
}

#[test]
fn supported_fallback_selection_has_no_extension_heuristic() {
    let face = parse(
        "font-family:A;src:local(Missing),url(bad.ttf)format(svg),\
        url(no-extension)format(woff2),url(last.ttf)",
        BASE,
    )
    .unwrap();
    assert_eq!(face.url, "https://example.test/css/no-extension");
    assert_eq!(
        parse("font-family:A;src:url(first)", BASE).unwrap().url,
        "https://example.test/css/first"
    );
}

#[test]
fn rule_discovery_uses_balanced_rules_instead_of_substring_search() {
    let faces = super::super::discover_font_faces(
        r#"
        /* @font-face{font-family:Comment;src:url(bad)} */
        p { content: '@font-face{font-family:String;src:url(bad)}'; }
        @font-face{font-family:"A}B";src:url("font)file.ttf")}
        @font-faceXXX{font-family:Bad;src:url(bad)}
        @media print { @font-face{font-family:Print;src:url(bad)} }
    "#,
        BASE,
    );
    assert_eq!(faces.len(), 1);
    assert_eq!(faces[0].family, "A}B");
    assert_eq!(faces[0].url, "https://example.test/css/font)file.ttf");
}
