//! Reservations never instantiate storage and retire with the final container.
use super::api_version_tests::{call, version_two};
use super::transform_delete_tests::retained_array;
use super::transform_feedback_tests::buffer;
use super::*;

fn native(context: &WebGl, id: i64) -> u32 {
    context.objects.get(id as u32, Kind::Buffer).unwrap().native
}

#[test]
fn webgl2_retired_buffer_reservation_handles_other_released_names() {
    session::run_native_test(|| {
        let mut context = version_two();
        let hole = buffer(&mut context, 8);
        let retained = buffer(&mut context, 8);
        let retired_name = native(&context, retained);
        let vao = retained_array(&mut context, retained);
        call(&mut context, "deleteBuffer", &[hole], "");
        let charge = context.resource_bytes;
        call(&mut context, "deleteBuffer", &[retained], "");
        assert_eq!(context.resource_bytes, charge);
        assert_eq!(unsafe { gl::IsBuffer(retired_name) }, gl::FALSE);
        for _ in 0..16 {
            let next = buffer(&mut context, 8);
            assert_ne!(native(&context, next), retired_name);
            call(&mut context, "deleteBuffer", &[next], "");
        }
        // Repeated deletion must not release the reservation early.
        call(&mut context, "deleteBuffer", &[retained], "");
        call(&mut context, "deleteVertexArray", &[vao], "");
        assert!(context.objects.get(retained as u32, Kind::Buffer).is_err());
        let first = buffer(&mut context, 8);
        let second = buffer(&mut context, 8);
        assert!([native(&context, first), native(&context, second)].contains(&retired_name));
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_retirement_failure_loses_only_its_native_owner() {
    session::run_native_test(|| {
        let mut contexts = BackendContexts::default();
        let owner = contexts.create(2, 2, r#"{"api":"webgl2"}"#).unwrap();
        let peer = contexts.create(2, 2, r#"{"api":"webgl2"}"#).unwrap();
        contexts.contexts.get_mut(&owner).unwrap().objects.poisoned = true;
        assert!(contexts.snapshot(owner).is_none());
        assert_eq!(contexts.complete_task(&[owner, peer]), vec![owner]);
        assert!(!contexts.contexts.contains_key(&owner));
        assert!(contexts.snapshot(peer).is_some());
        let owner = contexts.create(2, 2, r#"{"api":"webgl2"}"#).unwrap();
        contexts.contexts.get_mut(&owner).unwrap().objects.poisoned = true;
        assert_eq!(
            contexts.execute(owner, r#"{"op":"getError"}"#, None),
            json!({"lost":true})
        );
        assert!(!contexts.contexts.contains_key(&owner));
        assert!(contexts.snapshot(peer).is_some());
    });
}
