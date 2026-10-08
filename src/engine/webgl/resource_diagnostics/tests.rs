use super::*;
use crate::engine::webgl::{
    api_version_tests::{call, version_two},
    session,
};

#[test]
fn admission_snapshots_are_bounded_and_do_not_consume_author_errors() {
    session::run_native_test(|| {
        let mut context = version_two();
        let baseline = context.resource_bytes;
        context.resource_limit = baseline + 32;
        let id = call(&mut context, "createBuffer", &[], "")
            .as_i64()
            .unwrap();
        call(
            &mut context,
            "bindBuffer",
            &[gl::ARRAY_BUFFER as i64, id],
            "",
        );
        // prepare/commit does not call author code. Diagnostic failures are
        // recorded independently of WebGL's deduplicated sticky error set.
        assert_eq!(context.admit_storage_growth(33), Err(gl::OUT_OF_MEMORY));
        context.error(gl::OUT_OF_MEMORY);
        assert_eq!(
            context.admit_storage_growth(usize::MAX),
            Err(gl::OUT_OF_MEMORY)
        );
        let errors = context.errors.clone();
        for _ in 0..100 {
            let report = context.resource_diagnostic(17);
            assert!(report.starts_with("WebGL 17:"));
            assert!(report.contains("admission-rejections=2"));
            assert!(report.contains(&format!(
                "first-rejection(charged/additional/limit)={baseline}/33/{}",
                baseline + 32
            )));
            assert!(report.len() < 1024);
            assert_eq!(context.errors, errors);
            assert_eq!(context.resource_bytes, baseline);
        }
        assert_eq!(call(&mut context, "getError", &[], ""), gl::OUT_OF_MEMORY);
        assert_eq!(call(&mut context, "getError", &[], ""), gl::NO_ERROR);
        assert_eq!(context.admit_storage_growth(32), Ok(baseline + 32));
        assert_eq!(context.resource_diagnostics.rejections.get(), 2);
    });
}

#[test]
fn ledger_retains_first_and_latest_failure_without_an_unbounded_history() {
    let ledger = Ledger::default();
    for additional in 1..10_001 {
        assert_eq!(ledger.reject(100, additional, 100), gl::OUT_OF_MEMORY);
    }
    assert_eq!(ledger.rejections.get(), 10_000);
    assert_eq!(ledger.first.get().unwrap().additional, 1);
    assert_eq!(ledger.latest.get().unwrap().additional, 10_000);
    ledger.rejections.set(u64::MAX);
    ledger.reject(100, 10_001, 100);
    assert_eq!(ledger.rejections.get(), u64::MAX);
}

#[test]
fn snapshot_filters_peer_contexts_and_ignores_retired_ids() {
    session::run_native_test(|| {
        let mut backend = BackendContexts::default();
        // Test WARP contexts have the same ledger as the production owner.
        backend.contexts.insert(1, version_two());
        backend.contexts.insert(2, version_two());
        let report = backend.resource_diagnostics(&[1, 99]);
        assert_eq!(report.len(), 1);
        assert!(report[0].starts_with("WebGL 1:"));
        assert!(!report[0].contains("WebGL 2:"));
        backend.remove(1);
        assert!(backend.resource_diagnostics(&[1, 99]).is_empty());
    });
}
