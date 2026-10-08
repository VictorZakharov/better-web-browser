use super::*;
use crate::engine::webgl::api_version_tests::{call, version_two};
use crate::engine::webgl::{json, session};

fn buffer(context: &mut WebGl) -> u32 {
    let id = call(context, "createBuffer", &[], "").as_u64().unwrap() as u32;
    call(
        context,
        "bindBuffer",
        &[gl::ARRAY_BUFFER as i64, id as i64],
        "",
    );
    id
}

fn command(size: usize) -> Command {
    Command {
        op: "bufferData".into(),
        i: vec![
            gl::ARRAY_BUFFER as i64,
            size as i64,
            gl::DYNAMIC_DRAW as i64,
        ],
        f: vec![],
        text: String::new(),
    }
}

fn native(context: &mut WebGl, size: usize) -> Vec<u8> {
    // Invalidate the CPU mirror only in this test, forcing a real native map.
    let id = context.array_buffer;
    context
        .objects
        .get_mut(id, Kind::Buffer)
        .unwrap()
        .buffer_mirror_valid = false;
    context
        .read_buffer(&Command {
            op: "getBufferSubData".into(),
            i: vec![gl::ARRAY_BUFFER as i64, 0, size as i64],
            f: vec![],
            text: String::new(),
        })
        .unwrap()
}

#[test]
fn owned_buffer_upload_adopts_the_independent_allocation_after_native_success() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = buffer(&mut context);
        let bytes: Vec<u8> = (0..=255).cycle().take(4096).collect();
        let pointer = bytes.as_ptr();
        let expected = bytes.clone();
        let charge = context.resource_bytes;
        assert_eq!(
            context.dispatch_data(&command(bytes.len()), Some(Cow::Owned(bytes))),
            Ok(Value::Null)
        );
        let object = context.objects.get(id, Kind::Buffer).unwrap();
        assert_eq!(object.bytes.as_ptr(), pointer);
        assert_eq!(object.bytes, expected);
        assert!(object.buffer_mirror_valid);
        assert_eq!(context.resource_bytes, charge + expected.len() * 2);
        assert_eq!(native(&mut context, expected.len()), expected);
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn borrowed_buffer_upload_still_copies_and_numeric_storage_is_zeroed() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = buffer(&mut context);
        let mut source = vec![17; 128];
        context
            .dispatch(&command(source.len()), Some(&source))
            .unwrap();
        let object = context.objects.get(id, Kind::Buffer).unwrap();
        assert_ne!(object.bytes.as_ptr(), source.as_ptr());
        source.fill(99);
        assert_eq!(native(&mut context, 128), vec![17; 128]);
        context.dispatch_data(&command(256), None).unwrap();
        assert_eq!(native(&mut context, 256), vec![0; 256]);
        context
            .dispatch_data(&command(0), Some(Cow::Owned(vec![])))
            .unwrap();
        assert!(
            context
                .objects
                .get(id, Kind::Buffer)
                .unwrap()
                .bytes
                .is_empty()
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn rejected_owned_uploads_preserve_previous_native_bytes_mirror_and_accounting() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = buffer(&mut context);
        context
            .dispatch_data(&command(16), Some(Cow::Owned(vec![23; 16])))
            .unwrap();
        let pointer = context
            .objects
            .get(id, Kind::Buffer)
            .unwrap()
            .bytes
            .as_ptr();
        let charge = context.resource_bytes;
        let mut invalid_usage = command(16);
        invalid_usage.i[2] = 0;
        assert_eq!(
            context.dispatch_data(&invalid_usage, Some(Cow::Owned(vec![41; 16]))),
            Err(gl::INVALID_ENUM)
        );
        assert_eq!(
            context.dispatch_data(&command(32), Some(Cow::Owned(vec![41; 16]))),
            Err(gl::INVALID_VALUE)
        );
        context.resource_limit = charge;
        assert_eq!(
            context.dispatch_data(&command(32), Some(Cow::Owned(vec![41; 32]))),
            Err(gl::OUT_OF_MEMORY)
        );
        let object = context.objects.get(id, Kind::Buffer).unwrap();
        assert_eq!(object.bytes.as_ptr(), pointer);
        assert_eq!(object.bytes, vec![23; 16]);
        assert_eq!(context.resource_bytes, charge);
        assert_eq!(native(&mut context, 16), vec![23; 16]);
    });
}

#[test]
fn owned_upload_does_not_retain_excess_capacity_from_internal_callers() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = buffer(&mut context);
        let mut oversized = Vec::with_capacity(1024 * 1024);
        oversized.extend_from_slice(&[1, 2, 3, 4]);
        context
            .dispatch_data(&command(4), Some(Cow::Owned(oversized)))
            .unwrap();
        let object = context.objects.get(id, Kind::Buffer).unwrap();
        assert_eq!(object.bytes.capacity(), 4);
        assert_eq!(native(&mut context, 4), vec![1, 2, 3, 4]);
    });
}
