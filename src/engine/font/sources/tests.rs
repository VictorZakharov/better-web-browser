use super::*;

fn urls(source: &str) -> Vec<String> {
    parse(source)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|source| match source {
            FontSource::Url(url) => Some(url),
            FontSource::Local(_) => None,
        })
        .collect()
}

#[test]
fn source_urls_are_decoded_by_the_css_tokenizer_not_regular_expressions() {
    assert_eq!(
        urls(r#"u\72l("a\2c b;\)c.woff2") /* hint */ f\6frmat(w\6f ff2)"#),
        ["a,b;)c.woff2"]
    );
    assert_eq!(urls("url(/font?id=4)"), ["/font?id=4"]);
    assert_eq!(urls("URL('foo(1).ttf') FORMAT('TRUETYPE')"), ["foo(1).ttf"]);
    assert_eq!(
        urls("url(data:font/ttf;base64,AA==)"),
        ["data:font/ttf;base64,AA=="]
    );
    assert_eq!(urls(r"url(a\ b.woff)"), ["a b.woff"]);
}

#[test]
fn unknown_hints_never_fall_back_to_the_filename_extension() {
    for hint in [
        "format('unknown')",
        "format(svg)",
        "format(collection)",
        "tech(incremental)",
        "tech(variations)",
        "tech(color-COLRv1)",
        "format('woff2-variations')",
        "tech(features-opentype, features-aat)",
    ] {
        assert_eq!(
            urls(&format!("url(bad.ttf) {hint}, url(good) format(woff2)")),
            ["good"],
            "{hint}"
        );
    }
    assert_eq!(
        urls("url(a) format(opentype) tech(features-opentype)"),
        ["a"]
    );
}

#[test]
fn each_invalid_component_recovers_at_its_own_top_level_comma() {
    for invalid in [
        "'naked.ttf'",
        "url('a') bogus",
        "local(serif)",
        "url('a') format(woff,woff2)",
        "url('a') format()",
        "local(A,B)",
        "url('a') tech()",
        "url('a') format(woff) format(woff2)",
    ] {
        assert_eq!(
            urls(&format!("{invalid},url('okay,with,commas')")),
            ["okay,with,commas"],
            "{invalid}"
        );
    }
    assert_eq!(
        parse("local('Missing, Font'), url(next)"),
        Some(vec![
            FontSource::Local("Missing, Font".into()),
            FontSource::Url("next".into())
        ])
    );
    assert_eq!(
        parse(r"local(Some\ Font)"),
        Some(vec![FontSource::Local("Some Font".into())])
    );
}

#[test]
fn hostile_source_lists_are_bounded_without_partial_admission() {
    assert!(parse(&" ".repeat(MAX_SOURCE_BYTES + 1)).is_none());
    assert!(parse(&vec!["url(a)"; MAX_SOURCES + 1].join(",")).is_none());
    assert_eq!(
        parse(&vec!["url(a)"; MAX_SOURCES].join(",")).unwrap().len(),
        MAX_SOURCES
    );
    for value in ["", "bogus", "url('a') format(unknown)"] {
        assert!(parse(value).is_none(), "{value}");
    }
}
