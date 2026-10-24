//! Real data writes and draws, not an assumption that a cached range is immutable.
use super::api_version_tests::{call, version_two};
use super::core_uniform_tests::program;
use super::*;

const VERTEX: &str = "#version 300 es\nlayout(location=0) in vec2 position;void main(){gl_Position=vec4(position,0,1);}";
const FRAGMENT: &str =
    "#version 300 es\nprecision mediump float;out vec4 color;void main(){color=vec4(0,1,0,1);}";

fn upload(
    context: &mut WebGl,
    op: &str,
    target: u32,
    argument: usize,
    bytes: Option<&[u8]>,
) -> Result<Value> {
    context.dispatch(
        &Command {
            op: op.into(),
            i: if op == "bufferData" {
                vec![target as i64, argument as i64, gl::DYNAMIC_DRAW as i64]
            } else {
                vec![target as i64, argument as i64]
            },
            f: vec![],
            text: String::new(),
        },
        bytes,
    )
}

fn indices(context: &mut WebGl, values: &[u16]) -> u32 {
    let id = call(context, "createBuffer", &[], "").as_u64().unwrap() as u32;
    call(
        context,
        "bindBuffer",
        &[gl::ELEMENT_ARRAY_BUFFER as i64, id as i64],
        "",
    );
    replace(context, values);
    id
}

fn replace(context: &mut WebGl, values: &[u16]) {
    let bytes: Vec<_> = values.iter().flat_map(|v| v.to_ne_bytes()).collect();
    upload(
        context,
        "bufferData",
        gl::ELEMENT_ARRAY_BUFFER,
        bytes.len(),
        Some(&bytes),
    )
    .unwrap();
}

fn setup(context: &mut WebGl) {
    program(context, VERTEX, FRAGMENT);
    let buffer = call(context, "createBuffer", &[], "").as_i64().unwrap();
    call(
        context,
        "bindBuffer",
        &[gl::ARRAY_BUFFER as i64, buffer],
        "",
    );
    let bytes: Vec<_> = [-1f32, -1., 3., -1., -1., 3.]
        .iter()
        .flat_map(|v| v.to_ne_bytes())
        .collect();
    upload(
        context,
        "bufferData",
        gl::ARRAY_BUFFER,
        bytes.len(),
        Some(&bytes),
    )
    .unwrap();
    call(
        context,
        "vertexAttribPointer",
        &[0, 2, gl::FLOAT as i64, 0, 0, 0],
        "",
    );
    call(context, "enableVertexAttribArray", &[0], "");
}

fn draw(context: &mut WebGl) -> Result<Value> {
    context.dispatch(
        &Command {
            op: "drawElements".into(),
            i: vec![gl::TRIANGLES as i64, 3, gl::UNSIGNED_SHORT as i64, 0],
            f: vec![],
            text: String::new(),
        },
        None,
    )
}

#[test]
fn repeated_native_indexed_draws_scan_unchanged_bytes_once_and_render_pixels() {
    session::run_native_test(|| {
        let mut context = version_two();
        setup(&mut context);
        indices(&mut context, &[0, 1, 2]);
        for _ in 0..200 {
            assert!(draw(&mut context).is_ok());
        }
        assert_eq!(context.index_cache.scanned_bytes, 6);
        assert!(
            context
                .surface
                .snapshot()
                .unwrap()
                .chunks_exact(4)
                .all(|pixel| pixel == [0, 255, 0, 255])
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn partial_native_upload_invalidates_every_cached_range_of_its_buffer() {
    session::run_native_test(|| {
        let mut context = version_two();
        setup(&mut context);
        indices(&mut context, &[0, 1, 2, 0, 1, 2]);
        assert_eq!(context.maximum_index(3, 2, 0), Ok(Some(2)));
        assert_eq!(context.maximum_index(3, 2, 6), Ok(Some(2)));
        upload(
            &mut context,
            "bufferSubData",
            gl::ELEMENT_ARRAY_BUFFER,
            4,
            Some(&99u16.to_ne_bytes()),
        )
        .unwrap();
        assert_eq!(context.maximum_index(3, 2, 0), Ok(Some(99)));
        assert_eq!(draw(&mut context), Err(gl::INVALID_OPERATION));
        assert_eq!(context.maximum_index(3, 2, 6), Ok(Some(2)));
        assert_eq!(context.index_cache.scanned_bytes, 24);
        upload(
            &mut context,
            "bufferSubData",
            gl::ELEMENT_ARRAY_BUFFER,
            4,
            Some(&2u16.to_ne_bytes()),
        )
        .unwrap();
        assert!(draw(&mut context).is_ok());
        assert_eq!(context.index_cache.scanned_bytes, 30);
    });
}

#[test]
fn storage_replacement_and_numeric_zeroing_cannot_reuse_previous_maxima() {
    session::run_native_test(|| {
        let mut context = version_two();
        indices(&mut context, &[10, 20, 30]);
        assert_eq!(context.maximum_index(3, 2, 0), Ok(Some(30)));
        replace(&mut context, &[1, 2, 3]);
        assert_eq!(context.maximum_index(3, 2, 0), Ok(Some(3)));
        upload(
            &mut context,
            "bufferData",
            gl::ELEMENT_ARRAY_BUFFER,
            6,
            None,
        )
        .unwrap();
        assert_eq!(context.maximum_index(3, 2, 0), Ok(Some(0)));
        replace(&mut context, &[1]);
        assert_eq!(context.maximum_index(3, 2, 0), Err(gl::INVALID_OPERATION));
        assert_eq!(context.maximum_index(1, 2, 0), Ok(Some(1)));
    });
}

#[test]
fn native_buffer_copy_invalidates_destination_ranges_but_not_source_ranges() {
    session::run_native_test(|| {
        let mut context = version_two();
        let source = indices(&mut context, &[4, 5, 6]);
        assert_eq!(context.maximum_index(3, 2, 0), Ok(Some(6)));
        let destination = indices(&mut context, &[1, 2, 3]);
        assert_eq!(context.maximum_index(3, 2, 0), Ok(Some(3)));
        call(
            &mut context,
            "bindBuffer",
            &[core_buffers::COPY_READ as i64, source as i64],
            "",
        );
        call(
            &mut context,
            "copyBufferSubData",
            &[
                core_buffers::COPY_READ as i64,
                gl::ELEMENT_ARRAY_BUFFER as i64,
                0,
                0,
                6,
            ],
            "",
        );
        assert_eq!(context.maximum_index(3, 2, 0), Ok(Some(6)));
        let scanned = context.index_cache.scanned_bytes;
        call(
            &mut context,
            "bindBuffer",
            &[gl::ELEMENT_ARRAY_BUFFER as i64, source as i64],
            "",
        );
        assert_eq!(context.maximum_index(3, 2, 0), Ok(Some(6)));
        assert_eq!(context.index_cache.scanned_bytes, scanned);
        assert_eq!(
            context
                .objects
                .get(destination, Kind::Buffer)
                .unwrap()
                .bytes,
            [4u16, 5, 6]
                .iter()
                .flat_map(|v| v.to_ne_bytes())
                .collect::<Vec<_>>()
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn cached_index_maximum_still_checks_changed_vertex_buffer_and_vao_state() {
    session::run_native_test(|| {
        let mut context = version_two();
        setup(&mut context);
        let index = indices(&mut context, &[0, 1, 2]);
        assert!(draw(&mut context).is_ok());
        upload(&mut context, "bufferData", gl::ARRAY_BUFFER, 8, None).unwrap();
        assert_eq!(draw(&mut context), Err(gl::INVALID_OPERATION));
        let vao = call(&mut context, "createVertexArray", &[], "")
            .as_i64()
            .unwrap();
        call(&mut context, "bindVertexArray", &[vao], "");
        call(
            &mut context,
            "bindBuffer",
            &[gl::ELEMENT_ARRAY_BUFFER as i64, index as i64],
            "",
        );
        call(&mut context, "enableVertexAttribArray", &[0], "");
        assert_eq!(draw(&mut context), Err(gl::INVALID_OPERATION));
        assert_eq!(context.index_cache.scanned_bytes, 6);
    });
}

#[test]
fn cache_retains_webgl2_fixed_restart_semantics_for_every_index_width() {
    session::run_native_test(|| {
        for api in [ApiVersion::One, ApiVersion::Two] {
            let mut context = WebGl::new(
                4,
                4,
                Options {
                    api,
                    ..Options::default()
                },
            )
            .unwrap();
            let id = call(&mut context, "createBuffer", &[], "")
                .as_i64()
                .unwrap();
            call(
                &mut context,
                "bindBuffer",
                &[gl::ELEMENT_ARRAY_BUFFER as i64, id],
                "",
            );
            for width in [1, 2, 4] {
                let data = vec![255; width * 3];
                upload(
                    &mut context,
                    "bufferData",
                    gl::ELEMENT_ARRAY_BUFFER,
                    data.len(),
                    Some(&data),
                )
                .unwrap();
                let marker = match width {
                    1 => 255,
                    2 => 65535,
                    _ => u32::MAX,
                };
                let expected = if api == ApiVersion::Two {
                    None
                } else {
                    Some(marker)
                };
                for _ in 0..3 {
                    assert_eq!(context.maximum_index(3, width, 0), Ok(expected));
                }
                assert_eq!(
                    context.index_cache.lookup(index_cache::Key {
                        owner: id as u32,
                        offset: 0,
                        count: 3,
                        width
                    }),
                    Some(expected)
                );
            }
        }
    });
}

#[test]
fn rejected_native_writes_preserve_the_existing_range_cache_and_pixels() {
    session::run_native_test(|| {
        let mut context = version_two();
        setup(&mut context);
        indices(&mut context, &[0, 1, 2]);
        assert!(draw(&mut context).is_ok());
        assert_eq!(
            upload(
                &mut context,
                "bufferSubData",
                gl::ELEMENT_ARRAY_BUFFER,
                6,
                Some(&[9, 9])
            ),
            Err(gl::INVALID_VALUE)
        );
        assert_eq!(
            upload(
                &mut context,
                "bufferData",
                gl::ELEMENT_ARRAY_BUFFER,
                8,
                Some(&[9, 9])
            ),
            Err(gl::INVALID_VALUE)
        );
        assert!(draw(&mut context).is_ok());
        assert_eq!(context.index_cache.scanned_bytes, 6);
        assert!(
            context
                .surface
                .snapshot()
                .unwrap()
                .chunks_exact(4)
                .all(|p| p == [0, 255, 0, 255])
        );
    });
}

#[test]
fn invalid_cpu_mirror_refreshes_native_truth_before_consulting_cached_indices() {
    session::run_native_test(|| {
        let mut context = version_two();
        setup(&mut context);
        let id = indices(&mut context, &[0, 1, 2]);
        assert!(draw(&mut context).is_ok());
        let value = 99u16.to_ne_bytes();
        // Test-only direct native write models a future GPU route without
        // author buffer-class admission. Production writers invalidate eagerly.
        unsafe {
            gl::BufferSubData(gl::ELEMENT_ARRAY_BUFFER, 4, 2, value.as_ptr().cast());
        }
        context.driver_result().unwrap();
        context
            .objects
            .get_mut(id, Kind::Buffer)
            .unwrap()
            .buffer_mirror_valid = false;
        assert_eq!(context.maximum_index(3, 2, 0), Ok(Some(99)));
        assert_eq!(draw(&mut context), Err(gl::INVALID_OPERATION));
        assert_eq!(context.index_cache.scanned_bytes, 12);
        assert!(
            context
                .objects
                .get(id, Kind::Buffer)
                .unwrap()
                .buffer_mirror_valid
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}
