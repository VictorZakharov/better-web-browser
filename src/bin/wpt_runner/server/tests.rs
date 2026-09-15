use super::*;

#[test]
fn serves_upstreams_legacy_web_idl_parser_alias_without_rewriting_source() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("breeze-wpt-alias-{}-{nonce}", std::process::id()));
    let library = root.join("resources/webidl2/lib");
    std::fs::create_dir_all(&library).unwrap();
    let source = b"/* owned fixture bytes */";
    std::fs::write(library.join("webidl2.js"), source).unwrap();
    let response = route(
        &root.canonicalize().unwrap(),
        &[],
        "/resources/WebIDLParser.js",
    );
    assert_eq!(response.status, 200);
    assert_eq!(response.body, source);
    assert_eq!(response.content_type, "text/javascript; charset=utf-8");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_plain_and_encoded_traversal() {
    assert!(safe_relative_path("/../secret").is_err());
    assert!(safe_relative_path("/%2e%2e/secret").is_err());
    assert!(safe_relative_path("/..%5csecret").is_err());
}

#[test]
fn accepts_an_upstream_resource_path() {
    assert_eq!(
        safe_relative_path("/resources/testharness.js").unwrap(),
        PathBuf::from("resources/testharness.js")
    );
}

#[test]
fn recognizes_only_numeric_wrapper_routes() {
    assert_eq!(wrapper_index("/__breeze_wpt/12.html"), Some(12));
    assert_eq!(wrapper_index("/__breeze_wpt/test.html"), None);
}
