use super::*;
use crate::engine::webgl::{ApiVersion, Options, WebGl, api_version_tests::version_two, session};

fn numeric(id: u32, op: &str, integers: &[i64], floats: &[f64]) -> (u32, Entry) {
    (
        id,
        Entry::Numeric(NumericCommand::new(op.into(), integers.into(), floats.into()).unwrap()),
    )
}
fn color(id: u32, red: f64, green: f64) -> (u32, Entry) {
    numeric(id, "clearColor", &[], &[red, green, 0., 1.])
}
fn clear(id: u32) -> (u32, Entry) {
    numeric(id, "clear", &[gl::COLOR_BUFFER_BIT as i64], &[])
}
fn pixel(backend: &mut BackendContexts, id: u32) -> [u8; 4] {
    let (_, _, bytes) = backend.snapshot(id).unwrap();
    bytes[..4].try_into().unwrap()
}
fn error(backend: &mut BackendContexts, id: u32) -> Value {
    backend.execute(id, r#"{"op":"getError"}"#, None)
}

#[test]
fn contiguous_numeric_setters_activate_once_without_deduplicating_commands() {
    session::run_native_test(|| {
        let mut backend = BackendContexts::default();
        backend.contexts.insert(1, version_two());
        backend.set_profiling(&[1], true);
        let mut commands = Vec::new();
        for index in 0..64 {
            commands.push(color(1, (index % 2) as f64, (1 - index % 2) as f64));
            commands.push(clear(1));
        }
        assert!(backend.execute_batch(commands).is_empty());
        assert_eq!(backend.activation_checks, 1);
        let report = backend.resource_diagnostics(&[1]).join("\n");
        assert!(report.contains("state: 64 calls"), "{report}");
        assert!(report.contains("draw/clear: 64 calls"), "{report}");
        assert_eq!(pixel(&mut backend, 1), [255, 0, 0, 255]);
        assert_eq!(error(&mut backend, 1), json!(0));
    });
}

#[test]
fn interleaved_gl_versions_bind_each_group_to_its_own_surface_and_state() {
    session::run_native_test(|| {
        let mut backend = BackendContexts::default();
        backend.contexts.insert(
            1,
            WebGl::new(
                4,
                4,
                Options {
                    api: ApiVersion::One,
                    ..Options::default()
                },
            )
            .unwrap(),
        );
        backend.contexts.insert(2, version_two());
        assert!(
            backend
                .execute_batch(vec![
                    color(1, 1., 0.),
                    clear(1),
                    color(2, 0., 1.),
                    clear(2),
                    color(1, 0., 0.),
                    clear(1),
                ])
                .is_empty()
        );
        assert_eq!(backend.activation_checks, 3);
        assert_eq!(pixel(&mut backend, 1), [0, 0, 0, 255]);
        assert_eq!(pixel(&mut backend, 2), [0, 255, 0, 255]);
        assert_eq!(error(&mut backend, 1), json!(0));
        assert_eq!(error(&mut backend, 2), json!(0));
    });
}

#[test]
fn every_batch_rechecks_after_external_peer_activation_and_destruction() {
    session::run_native_test(|| {
        let mut backend = BackendContexts::default();
        backend.contexts.insert(1, version_two());
        assert!(
            backend
                .execute_batch(vec![color(1, 1., 0.), clear(1)])
                .is_empty()
        );
        assert_eq!(backend.activation_checks, 1);
        let peer = version_two(); // Changes the native thread-local binding.
        assert!(
            backend
                .execute_batch(vec![color(1, 0., 1.), clear(1)])
                .is_empty()
        );
        assert_eq!(backend.activation_checks, 2);
        assert_eq!(pixel(&mut backend, 1), [0, 255, 0, 255]);
        drop(peer); // Teardown activates/unbinds the peer outside our dispatcher.
        assert!(
            backend
                .execute_batch(vec![color(1, 1., 0.), clear(1)])
                .is_empty()
        );
        assert_eq!(backend.activation_checks, 3);
        assert_eq!(pixel(&mut backend, 1), [255, 0, 0, 255]);
    });
}

#[test]
fn json_fallback_and_malformed_records_end_the_numeric_binding_proof() {
    session::run_native_test(|| {
        let mut backend = BackendContexts::default();
        backend.contexts.insert(1, version_two());
        backend.contexts.insert(2, version_two());
        assert!(
            backend
                .execute_batch(vec![
                    color(1, 1., 0.),
                    (
                        2,
                        Entry::Json(r#"{"op":"clearColor","f":[0,1,0,1]}"#.into())
                    ),
                    clear(2),
                    (2, Entry::Json("malformed".into())),
                    clear(1),
                ])
                .is_empty()
        );
        assert_eq!(backend.activation_checks, 4);
        assert_eq!(pixel(&mut backend, 1), [255, 0, 0, 255]);
        assert_eq!(pixel(&mut backend, 2), [0, 255, 0, 255]);
        assert_eq!(error(&mut backend, 2), json!(gl::INVALID_VALUE));
        assert_eq!(error(&mut backend, 2), json!(0));
        assert_eq!(error(&mut backend, 1), json!(0));
    });
}

#[test]
fn missing_and_poisoned_contexts_retire_only_their_own_remaining_commands() {
    session::run_native_test(|| {
        let mut backend = BackendContexts::default();
        backend.contexts.insert(1, version_two());
        backend.contexts.insert(2, version_two());
        backend.contexts.get_mut(&1).unwrap().objects.poisoned = true;
        let lost = backend.execute_batch(vec![
            color(2, 0., 1.),
            clear(2),
            color(999, 1., 0.),
            clear(2),
            color(1, 1., 0.),
            clear(1),
            color(999, 0., 0.),
            clear(2),
        ]);
        assert_eq!(lost, [1, 999]);
        assert_eq!(backend.activation_checks, 4);
        assert!(!backend.contexts.contains_key(&1));
        assert_eq!(pixel(&mut backend, 2), [0, 255, 0, 255]);
        assert_eq!(error(&mut backend, 2), json!(0));
    });
}

#[test]
fn ordinary_gl_errors_keep_order_and_do_not_skip_later_valid_setters() {
    session::run_native_test(|| {
        let mut backend = BackendContexts::default();
        backend.contexts.insert(1, version_two());
        assert!(
            backend
                .execute_batch(vec![
                    numeric(1, "enable", &[0], &[]),
                    color(1, 0., 1.),
                    numeric(1, "viewport", &[0, 0, -1, 4], &[]),
                    clear(1),
                ])
                .is_empty()
        );
        assert_eq!(backend.activation_checks, 1);
        assert_eq!(pixel(&mut backend, 1), [0, 255, 0, 255]);
        assert_eq!(error(&mut backend, 1), json!(gl::INVALID_ENUM));
        assert_eq!(error(&mut backend, 1), json!(gl::INVALID_VALUE));
        assert_eq!(error(&mut backend, 1), json!(0));
    });
}

#[test]
fn empty_batches_do_not_activate_or_change_current_state() {
    session::run_native_test(|| {
        let mut backend = BackendContexts::default();
        assert!(backend.execute_batch(Vec::new()).is_empty());
        assert_eq!(backend.activation_checks, 0);
        backend.contexts.insert(1, version_two());
        assert!(
            backend
                .execute_batch(vec![color(1, 0., 1.), clear(1)])
                .is_empty()
        );
        assert!(backend.execute_batch(Vec::new()).is_empty());
        assert_eq!(backend.activation_checks, 1);
        assert_eq!(pixel(&mut backend, 1), [0, 255, 0, 255]);
    });
}
