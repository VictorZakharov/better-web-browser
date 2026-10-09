use crate::engine::webgl::{
    BackendContexts, Kind, api_version_tests::version_two, gl, json, session,
};

#[test]
fn native_profile_does_not_consume_errors_change_results_or_query_peer_contexts() {
    session::run_native_test(|| {
        let mut backend = BackendContexts::default();
        backend.contexts.insert(1, version_two());
        backend.contexts.insert(2, version_two());
        backend.set_profiling(&[1], true);
        assert_eq!(backend.execute(1, r#"{"op":"getError"}"#, None), json!(0));
        backend.execute(1, r#"{"op":"bindBuffer","i":[0,0]}"#, None);
        let errors = backend.contexts[&1].errors.clone();
        assert!(!errors.is_empty());
        let reports = backend.resource_diagnostics(&[1]);
        let text = reports.join("\n");
        assert!(text.contains("WebGL 1 native-owner query: 1 calls"));
        assert!(text.contains("WebGL 1 native-owner state: 1 calls"));
        assert!(!text.contains("WebGL 2"));
        assert_eq!(backend.contexts[&1].errors, errors);
        assert_eq!(
            backend.execute(1, r#"{"op":"getError"}"#, None),
            json!(gl::INVALID_ENUM)
        );
        assert_eq!(backend.execute(1, r#"{"op":"getError"}"#, None), json!(0));
        assert_eq!(
            backend.resource_diagnostics(&[2]).len(),
            1,
            "unprofiled peer has only a ledger"
        );
        backend.set_profiling(&[1], false);
        assert_eq!(backend.resource_diagnostics(&[1]).len(), 1);
    });
}

#[test]
fn profiling_applies_to_future_and_existing_realm_contexts_without_author_options() {
    let mut realm = crate::engine::webgl::Contexts::default();
    assert!(
        realm
            .create(1, 1, r#"{"api":"webgl2","profiling":true}"#)
            .is_none(),
        "private profiling controls are not creation IPC attributes"
    );
    let first = realm.create(1, 1, r#"{"api":"webgl2"}"#).unwrap();
    assert_eq!(
        realm.resource_diagnostics().len(),
        1,
        "author JSON cannot enable a native profile"
    );
    realm.set_profiling(true);
    let second = realm.create(1, 1, r#"{"api":"webgl2"}"#).unwrap();
    for id in [first, second] {
        realm.execute(id, r#"{"op":"getParameter","i":[7938]}"#, None);
    }
    let text = realm.resource_diagnostics().join("\n");
    for id in [first, second] {
        assert!(
            text.contains(&format!("WebGL {id} native-owner query:")),
            "{text}"
        );
    }
    realm.set_profiling(false);
    assert_eq!(realm.resource_diagnostics().len(), 2);
    realm.clear();
    assert!(realm.resource_diagnostics().is_empty());
}

#[test]
fn rejected_storage_records_kind_without_admitting_bytes() {
    session::run_native_test(|| {
        let context = version_two();
        let original = context.resource_bytes;
        for kind in [
            Kind::Buffer,
            Kind::Texture,
            Kind::Renderbuffer,
            Kind::Shader,
        ] {
            assert_eq!(
                context.admit_storage_growth_for(Some(kind), usize::MAX),
                Err(gl::OUT_OF_MEMORY)
            );
            assert_eq!(context.resource_bytes, original);
        }
        let mut backend = BackendContexts::default();
        backend.contexts.insert(1, context);
        let report = backend.resource_diagnostics(&[1]).join("\n");
        assert!(report.contains("first-rejection-kind=buffer"));
        assert!(report.contains("latest-rejection-kind=shader"));
        assert!(report.contains("admission-rejections=4"));
    });
}
