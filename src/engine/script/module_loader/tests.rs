use super::{MAX_RESOLVED_SPECIFIERS, WebModuleLoader};

const BASE: &str = "https://example.test/app/main.mjs";

#[test]
fn resolves_bare_and_url_like_specifiers_through_import_maps() {
    let loader = WebModuleLoader::new();
    loader
        .install_import_map(
            r#"{"imports":{"pkg":"/vendor/pkg.mjs","/app/dep.mjs":"/vendor/dep.mjs"}}"#,
            BASE,
        )
        .unwrap();

    assert_eq!(
        loader.resolve(BASE, "pkg").unwrap(),
        "https://example.test/vendor/pkg.mjs"
    );
    assert_eq!(
        loader.resolve(BASE, "./dep.mjs").unwrap(),
        "https://example.test/vendor/dep.mjs"
    );
    assert_eq!(
        loader.resolve(BASE, "./other.mjs").unwrap(),
        "https://example.test/app/other.mjs"
    );
    assert!(loader.resolve(BASE, "missing").is_err());
}

#[test]
fn new_maps_add_rules_without_replacing_existing_rules() {
    let loader = WebModuleLoader::new();
    loader
        .install_import_map(r#"{"imports":{"pkg":"/first.mjs"}}"#, BASE)
        .unwrap();
    loader
        .install_import_map(
            r#"{"imports":{"pkg":"/second.mjs","extra":"/extra.mjs"}}"#,
            BASE,
        )
        .unwrap();

    assert_eq!(
        loader.resolve(BASE, "pkg").unwrap(),
        "https://example.test/first.mjs"
    );
    assert_eq!(
        loader.resolve(BASE, "extra").unwrap(),
        "https://example.test/extra.mjs"
    );
}

#[test]
fn later_more_specific_rules_and_scopes_apply_before_resolution() {
    let loader = WebModuleLoader::new();
    loader
        .install_import_map(
            r#"{"imports":{"pkg/":"/general/","other":"/global.mjs"}}"#,
            BASE,
        )
        .unwrap();
    loader
        .install_import_map(
            r#"{"imports":{"pkg/special/":"/special/"},"scopes":{"/app/":{"other":"/scoped.mjs"}}}"#,
            BASE,
        )
        .unwrap();

    assert_eq!(
        loader.resolve(BASE, "pkg/special/tool.mjs").unwrap(),
        "https://example.test/special/tool.mjs"
    );
    assert_eq!(
        loader.resolve(BASE, "other").unwrap(),
        "https://example.test/scoped.mjs"
    );
}

#[test]
fn successful_resolution_is_stable_after_later_map() {
    let loader = WebModuleLoader::new();
    assert_eq!(
        loader.resolve(BASE, "./dep.mjs").unwrap(),
        "https://example.test/app/dep.mjs"
    );
    assert_eq!(
        loader.resolve(BASE, "/app/dep.mjs").unwrap(),
        "https://example.test/app/dep.mjs"
    );

    loader
        .install_import_map(
            r#"{"imports":{"/app/dep.mjs":"/replacement.mjs","new":"/new.mjs"}}"#,
            BASE,
        )
        .unwrap();
    assert_eq!(
        loader.resolve(BASE, "./dep.mjs").unwrap(),
        "https://example.test/app/dep.mjs"
    );
    assert_eq!(
        loader.resolve(BASE, "new").unwrap(),
        "https://example.test/new.mjs"
    );
}

#[test]
fn invalid_map_is_atomic_and_clear_resets_document_state() {
    let loader = WebModuleLoader::new();
    loader
        .install_import_map(r#"{"imports":{"pkg":"/first.mjs"}}"#, BASE)
        .unwrap();
    assert!(loader.install_import_map("{", BASE).is_err());
    assert!(
        loader
            .install_import_map(r#"{"integrity":{"/first.mjs":"sha384-abc"}}"#, BASE,)
            .is_err()
    );
    assert_eq!(
        loader.resolve(BASE, "pkg").unwrap(),
        "https://example.test/first.mjs"
    );

    loader.clear();
    assert!(loader.resolve(BASE, "pkg").is_err());
    loader
        .install_import_map(r#"{"imports":{"pkg":"/second.mjs"}}"#, BASE)
        .unwrap();
    assert_eq!(
        loader.resolve(BASE, "pkg").unwrap(),
        "https://example.test/second.mjs"
    );
}

#[test]
fn url_like_specifiers_use_url_standard_resolution() {
    let loader = WebModuleLoader::new();
    assert_eq!(
        loader
            .resolve(BASE, "data:text/javascript,export%20default%201")
            .unwrap(),
        "data:text/javascript,export%20default%201"
    );
    assert_eq!(
        loader
            .resolve(BASE, "blob:https://example.test/id")
            .unwrap(),
        "blob:https://example.test/id"
    );
}

#[test]
fn settled_module_records_are_bounded() {
    let loader = WebModuleLoader::new();
    for index in 0..MAX_RESOLVED_SPECIFIERS {
        loader
            .resolve(BASE, &format!("./module-{index}.mjs"))
            .unwrap();
    }
    assert!(loader.resolve(BASE, "./overflow.mjs").is_err());
    assert_eq!(
        loader.resolve(BASE, "./module-0.mjs").unwrap(),
        "https://example.test/app/module-0.mjs"
    );
}
