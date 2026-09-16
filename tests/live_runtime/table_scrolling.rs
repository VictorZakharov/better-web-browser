#[test]
fn table_wrapper_client_boxes_and_enclosing_scroll_offsets_match_chromium() {
    let report = super::parser_writes::run_with_args(
        include_str!("../../benchmarks/alpha/fixtures/table-scroll-geometry.html"),
        &["--device-scale-factor", "1"],
    );
    assert_eq!(
        report["titles"]["document_title"],
        r#"[["top",[165,85,260,230,260,230],[95,145,95,145],[0,0]],["bottom",[165,85,260,230,260,230],[95,145,95,145],[0,0]],["collapsed",[165,85,260,230,260,230],[95,145,95,145],[0,0]],["css-table",[165,85,274,244,274,244],[109,159,109,159],[0,0]],["visible table offsets",0,0],["block table",254,174,3,3],["no box",0,0,0,0]]"#,
        "{report}"
    );
}
