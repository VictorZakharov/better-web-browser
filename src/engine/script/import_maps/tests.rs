use super::*;

const BASE: &str = "https://example.test/app/index.html";

fn parse(source: &str) -> ImportMap {
    ImportMap::parse(source, BASE).unwrap()
}

#[test]
fn exact_bare_and_url_like_keys_use_normalized_addresses() {
    let map = parse(
        r#"{"imports":{"library":"./vendor/library.js","/shared.js":"./local.js","https://cdn.test/core.js":"https://cdn.test/v2.js"}}"#,
    );
    assert_eq!(
        map.resolve("library", BASE),
        Ok(Some("https://example.test/app/vendor/library.js".into()))
    );
    assert_eq!(
        map.resolve("/shared.js", BASE),
        Ok(Some("https://example.test/app/local.js".into()))
    );
    assert_eq!(
        map.resolve("https://cdn.test/core.js", BASE),
        Ok(Some("https://cdn.test/v2.js".into()))
    );
    assert_eq!(map.resolve("unmapped", BASE), Ok(None));
    assert_eq!(map.resolve("./ordinary.js", BASE), Ok(None));
}

#[test]
fn author_order_decides_colliding_normalized_specifier_keys() {
    let map = parse(
        r#"{"imports":{"https://example.test/shared.js":"/first.js","/shared.js":"/second.js"}}"#,
    );
    assert_eq!(
        map.resolve("/shared.js", BASE),
        Ok(Some("https://example.test/second.js".into()))
    );
}

#[test]
fn author_order_decides_colliding_normalized_scope_urls() {
    let map = parse(
        r#"{"scopes":{"https://example.test/app/":{"dep":"/first.js"},"/app/":{"dep":"/second.js"}}}"#,
    );
    assert_eq!(
        map.resolve("dep", BASE),
        Ok(Some("https://example.test/second.js".into()))
    );
}

#[test]
fn longest_prefix_wins_and_directory_escape_is_blocked() {
    let map =
        parse(r#"{"imports":{"pkg/":"/vendor/","pkg/private/":"/secure/","pkg/exact":"/one.js"}}"#);
    assert_eq!(
        map.resolve("pkg/private/module.js", BASE),
        Ok(Some("https://example.test/secure/module.js".into()))
    );
    assert_eq!(
        map.resolve("pkg/other.js", BASE),
        Ok(Some("https://example.test/vendor/other.js".into()))
    );
    assert_eq!(
        map.resolve("pkg/exact", BASE),
        Ok(Some("https://example.test/one.js".into()))
    );
    assert!(map.resolve("pkg/../escape.js", BASE).is_err());
}

#[test]
fn scopes_use_most_specific_match_then_fall_back_to_imports() {
    let map = parse(
        r#"{"imports":{"shared":"/global.js"},"scopes":{"/app/":{"shared":"/app.js"},"/app/private/":{"secret":"/secret.js"}}}"#,
    );
    let private = "https://example.test/app/private/entry.js";
    assert_eq!(
        map.resolve("secret", private),
        Ok(Some("https://example.test/secret.js".into()))
    );
    assert_eq!(
        map.resolve("shared", private),
        Ok(Some("https://example.test/app.js".into()))
    );
    assert_eq!(
        map.resolve("shared", "https://example.test/app2/entry.js"),
        Ok(Some("https://example.test/global.js".into()))
    );
}

#[test]
fn null_and_invalid_addresses_block_without_less_specific_fallback() {
    let map = parse(
        r#"{"imports":{"blocked":"/global.js","broken":"bare-address","pkg/":"/without-trailing-slash"},"scopes":{"/app/":{"blocked":null}}}"#,
    );
    assert!(map.resolve("blocked", BASE).is_err());
    assert!(map.resolve("broken", BASE).is_err());
    assert!(map.resolve("pkg/file.js", BASE).is_err());
    assert_eq!(
        map.resolve("blocked", "https://example.test/outside.js"),
        Ok(Some("https://example.test/global.js".into()))
    );
    assert!(
        map.diagnostics()
            .iter()
            .any(|entry| entry.contains("broken"))
    );
}

#[test]
fn invalid_entries_are_diagnostic_but_do_not_discard_valid_rules() {
    let map = parse(
        r#"{"imports":{"":"/ignored.js","invalid":42,"valid":"/works.js"},"scopes":{"http://[":{},"/app/":{"scoped":"/yes.js"}},"extra":true}"#,
    );
    assert_eq!(
        map.resolve("valid", BASE),
        Ok(Some("https://example.test/works.js".into()))
    );
    assert_eq!(
        map.resolve("scoped", BASE),
        Ok(Some("https://example.test/yes.js".into()))
    );
    assert!(map.resolve("invalid", BASE).is_err());
    assert!(
        map.diagnostics()
            .iter()
            .any(|entry| entry.contains("scope"))
    );
    assert!(
        map.diagnostics()
            .iter()
            .any(|entry| entry.contains("extra"))
    );
}

#[test]
fn malformed_json_and_invalid_map_shapes_fail_the_whole_map() {
    for source in [
        "not JSON",
        "[]",
        r#"{"imports":[]} "#,
        r#"{"scopes":{"/app/":false}}"#,
        r#"{"integrity":false}"#,
    ] {
        assert!(ImportMap::parse(source, BASE).is_err(), "{source}");
    }
}

#[test]
fn integrity_metadata_is_retained_and_unknown_values_are_diagnostic() {
    let map =
        parse(r#"{"integrity":{"/module.js":"sha256-abc","bare": "ignored","/other.js":42}}"#);
    assert!(map.has_integrity());
    assert_eq!(
        map.integrity.get("https://example.test/module.js"),
        Some(&"sha256-abc".to_owned())
    );
    assert!(!map.integrity.contains_key("https://example.test/other.js"));
    assert!(!map.integrity.contains_key("bare"));
    assert_eq!(map.diagnostics().len(), 2);
}

#[test]
fn non_special_urls_allow_exact_but_not_prefix_remapping() {
    let map = parse(r#"{"imports":{"data:text/":"/mapped/","data:text/plain,ok":"/exact.js"}}"#);
    assert_eq!(
        map.resolve("data:text/plain,ok", BASE),
        Ok(Some("https://example.test/exact.js".into()))
    );
    assert_eq!(map.resolve("data:text/plain,other", BASE), Ok(None));
}

#[test]
fn source_and_entry_counts_are_bounded() {
    assert!(ImportMap::parse(&" ".repeat(MAX_IMPORT_MAP_BYTES + 1), BASE).is_err());
    let imports = (0..=MAX_IMPORT_MAP_ENTRIES)
        .map(|index| format!("\"m{index}\":\"/m{index}.js\""))
        .collect::<Vec<_>>()
        .join(",");
    assert!(ImportMap::parse(&format!("{{\"imports\":{{{imports}}}}}"), BASE).is_err());
}

#[test]
fn later_maps_add_specific_rules_without_overriding_old_exact_keys() {
    let mut map = parse(
        r#"{"imports":{"pkg/":"/old/","same":"/first.js"},"scopes":{"/app/":{"same":"/scoped-first.js"}},"integrity":{"/same.js":"sha256-first"}}"#,
    );
    map.merge(parse(
        r#"{"imports":{"pkg/":"/wrong/","pkg/ui/":"/new/","same":"/wrong.js"},"scopes":{"/app/":{"same":"/wrong.js","new":"/scoped-new.js"},"/app/private/":{"private":"/private.js"}},"integrity":{"/same.js":"sha256-wrong","/new.js":"sha256-new"}}"#,
    ), &[])
    .unwrap();

    assert_eq!(
        map.resolve("pkg/old.js", BASE),
        Ok(Some("https://example.test/old/old.js".into()))
    );
    assert_eq!(
        map.resolve("pkg/ui/new.js", BASE),
        Ok(Some("https://example.test/new/new.js".into()))
    );
    assert_eq!(
        map.resolve("same", BASE),
        Ok(Some("https://example.test/scoped-first.js".into()))
    );
    assert_eq!(
        map.resolve("new", BASE),
        Ok(Some("https://example.test/scoped-new.js".into()))
    );
    assert_eq!(
        map.resolve("private", "https://example.test/app/private/main.js"),
        Ok(Some("https://example.test/private.js".into()))
    );
    assert_eq!(
        map.integrity.get("https://example.test/same.js"),
        Some(&"sha256-first".to_owned())
    );
    assert_eq!(
        map.integrity.get("https://example.test/new.js"),
        Some(&"sha256-new".to_owned())
    );
}

#[test]
fn late_maps_cannot_retarget_already_resolved_specifiers() {
    let mut map = parse(r#"{"imports":{"pkg/":"/old/"}}"#);
    let settled = [ResolvedSpecifier::new(BASE, "pkg/module.js").unwrap()];
    map.merge(
        parse(
            r#"{"imports":{"pkg/module.js":"/late-exact.js","pkg/":"/late-prefix/","other":"/new.js"},"scopes":{"/app/":{"pkg/module.js":"/late-scoped.js","other":"/scoped-new.js"},"/elsewhere/":{"pkg/module.js":"/elsewhere.js"}}}"#,
        ),
        &settled,
    )
    .unwrap();

    assert_eq!(
        map.resolve("pkg/module.js", BASE),
        Ok(Some("https://example.test/old/module.js".into()))
    );
    assert_eq!(
        map.resolve("other", BASE),
        Ok(Some("https://example.test/scoped-new.js".into()))
    );
    assert_eq!(
        map.resolve("pkg/module.js", "https://example.test/elsewhere/entry.js"),
        Ok(Some("https://example.test/elsewhere.js".into()))
    );
}

#[test]
fn resolved_url_like_specifiers_filter_normalized_late_keys() {
    let mut map = parse("{}");
    let settled = [ResolvedSpecifier::new(BASE, "./module.js").unwrap()];
    map.merge(
        parse(r#"{"imports":{"/app/module.js":"/late.js","new":"/new.js"}}"#),
        &settled,
    )
    .unwrap();
    assert_eq!(map.resolve("./module.js", BASE), Ok(None));
    assert_eq!(
        map.resolve("new", BASE),
        Ok(Some("https://example.test/new.js".into()))
    );
}
