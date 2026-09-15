use super::*;
use crate::engine::css::StylesheetSource;

#[test]
fn import_conditions_ignore_comments_without_joining_tokens_or_rewriting_strings() {
    let imports = parse(
        "@import 'a' supports((display:/**/block)) screen/**/and (min-width:/**/1px); @import 'b' scr/**/een;",
    );
    let environment = MediaEnvironment::new(800.0, 600.0, 1.0, false);
    assert!(imports[0].matches(environment));
    assert!(!imports[1].matches(environment));
}

fn sheet(name: &str, source: &str) -> StylesheetSource {
    StylesheetSource::linked(&format!("https://example.test/{name}"), source.into())
}

#[test]
fn nested_repeated_and_diamond_imports_have_independent_cascade_positions() {
    let sources = [
        sheet("a.css", "@import 'shared.css';"),
        sheet("b.css", "@import 'shared.css';"),
        sheet("shared.css", "p{color:red}"),
    ];
    let expanded = expand(
        "https://example.test/root.css",
        &parse("@import 'a.css'; @import 'b.css'; @import 'a.css';"),
        &sources,
        MediaEnvironment::new(800.0, 600.0, 1.0, false),
    );
    assert_eq!(
        expanded
            .sheets
            .iter()
            .map(|s| s.url().rsplit('/').next().unwrap())
            .collect::<Vec<_>>(),
        [
            "shared.css",
            "a.css",
            "shared.css",
            "b.css",
            "shared.css",
            "a.css"
        ]
    );
    assert!(!expanded.truncated);
}

#[test]
fn cycles_fragments_and_redirect_bases_are_handled_without_recursive_fetches() {
    let a = sheet("a.css", "@import 'b.css#one';");
    let b = sheet("b.css", "@import 'a.css#two';");
    let sources = [a, b];
    let environment = MediaEnvironment::new(800.0, 600.0, 1.0, false);
    let result = expand(
        &sources[0].base_url,
        &sources[0].imports,
        &sources,
        environment,
    );
    assert_eq!(result.urls, ["https://example.test/b.css"]);
    assert!(!result.truncated);
    let mut redirected = sheet("redirect.css", "@import 'leaf.css';");
    redirected.base_url = "https://cdn.test/theme/final.css".into();
    let sources = [redirected];
    let result = expand(
        "https://example.test/root.css",
        &parse("@import 'redirect.css';"),
        &sources,
        environment,
    );
    assert_eq!(
        result.urls,
        [
            "https://example.test/redirect.css",
            "https://cdn.test/theme/leaf.css"
        ]
    );
}

#[test]
fn occurrence_budget_bounds_exponential_import_expansion() {
    let sources = (0..12)
        .map(|n| {
            sheet(
                &format!("{n}.css"),
                &format!("@import '{}.css'; @import '{}.css';", n + 1, n + 1),
            )
        })
        .collect::<Vec<_>>();
    let result = expand(
        "https://example.test/root.css",
        &parse("@import '0.css';"),
        &sources,
        MediaEnvironment::new(800.0, 600.0, 1.0, false),
    );
    assert!(result.truncated);
    assert!(result.urls.len() <= MAX_IMPORT_OCCURRENCES);
    assert!(result.sheets.len() <= MAX_IMPORT_OCCURRENCES);
}
