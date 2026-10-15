use super::*;

fn parse(arguments: &[&str]) -> Result<LaunchOptions, String> {
    LaunchOptions::parse_from(
        Instant::now(),
        arguments.iter().map(|value| (*value).to_owned()),
    )
}

#[test]
fn title_completion_is_explicit_bounded_and_exclusive() {
    let options = parse(&[
        "--benchmark",
        "about:blank",
        "--output",
        "result.json",
        "--completion-title",
        "ready",
    ])
    .unwrap();
    let mut run = options.benchmark.unwrap();
    assert!(!run.record_document_title("not ready"));
    assert!(!run.record_script_console(&["log: ready".into()]));
    assert!(run.record_document_title("ready"));
    assert!(!run.record_document_title("ready"));
    assert!(!run.record_document_title("later"));
    assert_eq!(run.titles.json("browser")["document_title"], "later");
    assert!(parse(&["--completion-title", "ready"]).is_err());
    assert!(
        parse(&[
            "--benchmark",
            "about:blank",
            "--output",
            "result.json",
            "--completion-title",
            "ready",
            "--completion-marker",
            "ready"
        ])
        .is_err()
    );
    let over_limit = "x".repeat(diagnostics::MAX_DIAGNOSTIC_TITLE_BYTES + 1);
    assert!(
        parse(&[
            "--benchmark",
            "about:blank",
            "--output",
            "result.json",
            "--completion-title",
            &over_limit
        ])
        .is_err()
    );
}

#[test]
fn title_completion_uses_exact_dom_text_and_accepts_an_empty_title() {
    let mut run = parse(&[
        "--benchmark",
        "about:blank",
        "--output",
        "result.json",
        "--completion-title",
        "",
    ])
    .unwrap()
    .benchmark
    .unwrap();
    assert!(!run.record_document_title(" "));
    assert!(run.record_document_title(""));
}
