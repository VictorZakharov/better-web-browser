#[test]
fn dynamic_inline_scripts_follow_post_connection_order_in_the_retained_parser() {
    let report = super::parser_writes::run(include_str!(
        "../../benchmarks/alpha/fixtures/inline-script-insertion.html"
    ));
    assert_eq!(
        report["titles"]["document_title"],
        r#"[["nested","nested"],["inner","inner"],["restored","nested"],["returned","outer"],["filled","filled"],["atomic",true],["replacement",true],["child text"],["shadow",true],["job","outer"]]"#,
        "{report}"
    );
}
