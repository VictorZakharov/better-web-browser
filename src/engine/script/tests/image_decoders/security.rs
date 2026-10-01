//! Secure-context exposure is separate from image codec capability.
use super::super::*;

fn run_at(url: &str) -> Vec<String> {
    let dom = dom::parse_with_scripting(
        r#"<script>
        console.log(typeof ImageDecoder+'|'+typeof ImageTrack+'|'+typeof ImageTrackList+'|'+typeof VideoFrame);
    </script>"#,
        true,
    );
    let node = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), url);
    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: format!("{url}#inline"),
        code: node.text_content(),
        node,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    outcome.console
}

#[test]
fn secure_and_loopback_documents_expose_image_decoder_but_plain_http_does_not() {
    for url in [
        "https://example.test/",
        "http://127.0.0.1/",
        "http://localhost/",
    ] {
        assert_eq!(
            run_at(url),
            ["log: function|function|function|function"],
            "{url}"
        );
    }
    assert_eq!(
        run_at("http://example.test/"),
        ["log: undefined|undefined|undefined|function"]
    );
}

#[test]
fn insecure_page_still_constructs_canvas_frame_without_exposing_decoder() {
    let dom = dom::parse_with_scripting(
        r#"<script>
        const frame=new VideoFrame(new Uint8Array([1,2,3,255]),{format:'RGBA',codedWidth:1,codedHeight:1,timestamp:0});
        console.log(frame.codedWidth);frame.close();
    </script>"#,
        true,
    );
    let node = dom.elements_named("script").next().unwrap();
    let outcome = execute(
        dom.document.clone(),
        "http://example.test/",
        &[ScriptInput {
            source_url: "http://example.test/#inline".into(),
            code: node.text_content(),
            node,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        }],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: 1"]);
}
