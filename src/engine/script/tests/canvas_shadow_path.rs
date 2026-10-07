use super::*;

const SHADOW_PATH: &str = include_str!("../../../../tests/canvas/shadow-path.js");

#[test]
fn fused_shadow_geometry_equals_filtered_identity_source_layers_in_document_realms() {
    for mode in [
        "source-over",
        "source-in",
        "source-out",
        "source-atop",
        "destination-over",
        "destination-in",
        "destination-out",
        "destination-atop",
        "xor",
        "copy",
        "lighter",
        "multiply",
        "screen",
        "overlay",
        "darken",
        "lighten",
        "color-dodge",
        "color-burn",
        "hard-light",
        "soft-light",
        "difference",
        "exclusion",
        "hue",
        "saturation",
        "color",
        "luminosity",
    ] {
        let (_, outcome) = execute_html(&format!(
            "<script>{SHADOW_PATH}\nif(testShadowPathPainting('{mode}')!==12)throw Error('missing cases');</script>"
        ));
        assert!(outcome.errors.is_empty(), "{mode}: {:?}", outcome.errors);
    }
}

#[test]
fn workers_use_the_same_fused_shadow_path_and_fallback_semantics() {
    let source = format!("{SHADOW_PATH}\npostMessage(testShadowPathPainting('destination-in'));");
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/shadow-path.js",
        &source,
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected worker fetch {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(runtime.is_some());
    assert_eq!(initial.messages, ["12"]);
}
