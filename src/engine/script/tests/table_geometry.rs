use super::*;

#[test]
fn table_client_boxes_follow_computed_display_and_used_border_widths() {
    let dom = dom::parse_with_scripting(
        r#"<!doctype html><body>
      <div id=css style="display:table;border:3px solid"></div>
      <table id=collapsed style="border:3px solid;border-collapse:collapse"></table>
      <table id=block style="display:block;border:3px solid"></table>
      <script>document.body.dataset.result=JSON.stringify([css,collapsed,block].map(t=>[t.clientWidth,t.clientHeight,t.clientLeft,t.clientTop]));</script>
    "#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let bounds = dom
        .elements_named("div")
        .chain(dom.elements_named("table"))
        .map(|node| {
            (
                node.id(),
                RectF {
                    x: 0.0,
                    y: 0.0,
                    width: 100.0,
                    height: 80.0,
                },
            )
        })
        .collect();
    runtime.set_layout_geometry(&bounds);
    let node = dom.elements_named("script").next().unwrap();
    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.com/".into(),
        code: node.text_content(),
        node,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("[[100,80,0,0],[100,80,0,0],[94,74,3,3]]")
    );
}
