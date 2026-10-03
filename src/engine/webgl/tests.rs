use super::*;

pub(super) fn context() -> (Contexts, u32) {
    let mut contexts = Contexts::default();
    let id = contexts
        .create(16, 16, "{}")
        .expect("headless ANGLE/WARP context");
    (contexts, id)
}
pub(super) fn command(
    contexts: &mut Contexts,
    id: u32,
    op: &str,
    i: &[u32],
    f: &[f32],
    text: &str,
    bytes: Option<&[u8]>,
) -> Value {
    contexts.execute(
        id,
        &json!({"op":op,"i":i,"f":f,"text":text}).to_string(),
        bytes,
    )
}
fn shader(contexts: &mut Contexts, id: u32, kind: u32, source: &str) -> u32 {
    let shader = command(contexts, id, "createShader", &[kind], &[], "", None)
        .as_u64()
        .unwrap() as u32;
    command(contexts, id, "shaderSource", &[shader], &[], source, None);
    command(contexts, id, "compileShader", &[shader], &[], "", None);
    assert_eq!(
        command(
            contexts,
            id,
            "getShaderParameter",
            &[shader, gl::COMPILE_STATUS],
            &[],
            "",
            None
        ),
        json!(true)
    );
    shader
}
#[test]
fn fresh_drawing_buffer_is_zero_initialized() {
    let (mut contexts, id) = context();
    let (_, _, pixels) = contexts.snapshot(id).unwrap();
    assert!(pixels.iter().all(|b| *b == 0));
}
#[test]
fn shader_triangle_produces_pixels_and_out_of_bounds_draw_does_not() {
    let (mut contexts, id) = context();
    let vertex = shader(
        &mut contexts,
        id,
        gl::VERTEX_SHADER,
        "attribute vec2 position; void main(){gl_Position=vec4(position,0.0,1.0);}",
    );
    let fragment = shader(
        &mut contexts,
        id,
        gl::FRAGMENT_SHADER,
        "precision mediump float; uniform vec4 color; void main(){gl_FragColor=color;}",
    );
    let program = command(&mut contexts, id, "createProgram", &[], &[], "", None)
        .as_u64()
        .unwrap() as u32;
    for shader in [vertex, fragment] {
        command(
            &mut contexts,
            id,
            "attachShader",
            &[program, shader],
            &[],
            "",
            None,
        );
    }
    command(&mut contexts, id, "linkProgram", &[program], &[], "", None);
    assert_eq!(
        command(
            &mut contexts,
            id,
            "getProgramParameter",
            &[program, gl::LINK_STATUS],
            &[],
            "",
            None
        ),
        json!(true)
    );
    command(&mut contexts, id, "useProgram", &[program], &[], "", None);
    let uniform = command(
        &mut contexts,
        id,
        "getUniformLocation",
        &[program],
        &[],
        "color",
        None,
    )
    .as_u64()
    .unwrap() as u32;
    command(
        &mut contexts,
        id,
        "uniform4f",
        &[uniform],
        &[1.0, 0.0, 0.0, 1.0],
        "",
        None,
    );
    let buffer = command(&mut contexts, id, "createBuffer", &[], &[], "", None)
        .as_u64()
        .unwrap() as u32;
    command(
        &mut contexts,
        id,
        "bindBuffer",
        &[gl::ARRAY_BUFFER, buffer],
        &[],
        "",
        None,
    );
    let data: Vec<u8> = [-1.0f32, -1.0, 1.0, -1.0, 0.0, 1.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    command(
        &mut contexts,
        id,
        "bufferData",
        &[gl::ARRAY_BUFFER, data.len() as u32, gl::STATIC_DRAW],
        &[],
        "",
        Some(&data),
    );
    let location = command(
        &mut contexts,
        id,
        "getAttribLocation",
        &[program],
        &[],
        "position",
        None,
    )
    .as_u64()
    .unwrap() as u32;
    command(
        &mut contexts,
        id,
        "vertexAttribPointer",
        &[location, 2, gl::FLOAT, 0, 0, 0],
        &[],
        "",
        None,
    );
    command(
        &mut contexts,
        id,
        "enableVertexAttribArray",
        &[location],
        &[],
        "",
        None,
    );
    command(
        &mut contexts,
        id,
        "clearColor",
        &[],
        &[0.0, 0.0, 1.0, 1.0],
        "",
        None,
    );
    command(
        &mut contexts,
        id,
        "clear",
        &[gl::COLOR_BUFFER_BIT],
        &[],
        "",
        None,
    );
    command(
        &mut contexts,
        id,
        "drawArrays",
        &[gl::TRIANGLES, 0, 4],
        &[],
        "",
        None,
    );
    assert_eq!(
        command(&mut contexts, id, "getError", &[], &[], "", None),
        json!(gl::INVALID_OPERATION)
    );
    assert_eq!(
        &contexts.snapshot(id).unwrap().2[544..548],
        &[0, 0, 255, 255]
    );
    command(
        &mut contexts,
        id,
        "drawArrays",
        &[gl::TRIANGLES, 0, 3],
        &[],
        "",
        None,
    );
    assert_eq!(
        command(&mut contexts, id, "getError", &[], &[], "", None),
        json!(gl::NO_ERROR)
    );
    assert_eq!(
        &contexts.snapshot(id).unwrap().2[544..548],
        &[255, 0, 0, 255]
    );
    command(&mut contexts, id, "linkProgram", &[program], &[], "", None);
    command(
        &mut contexts,
        id,
        "uniform4f",
        &[uniform],
        &[0.0, 1.0, 0.0, 1.0],
        "",
        None,
    );
    assert_eq!(
        command(&mut contexts, id, "getError", &[], &[], "", None),
        json!(gl::INVALID_OPERATION)
    );
    let fresh = command(
        &mut contexts,
        id,
        "getUniformLocation",
        &[program],
        &[],
        "color",
        None,
    )
    .as_u64()
    .unwrap() as u32;
    assert_ne!(uniform, fresh);
    command(
        &mut contexts,
        id,
        "uniform4f",
        &[fresh],
        &[0.0, 1.0, 0.0, 1.0],
        "",
        None,
    );
    command(
        &mut contexts,
        id,
        "drawArrays",
        &[gl::TRIANGLES, 0, 3],
        &[],
        "",
        None,
    );
    assert_eq!(
        &contexts.snapshot(id).unwrap().2[544..548],
        &[0, 255, 0, 255]
    );
}
#[test]
fn malformed_commands_and_wrong_object_types_fail_closed() {
    let (mut contexts, id) = context();
    contexts.execute(id, "{\"op\":\"clear\",\"i\":[-1]}", None);
    assert_eq!(
        command(&mut contexts, id, "getError", &[], &[], "", None),
        json!(gl::INVALID_VALUE)
    );
    let shader = command(
        &mut contexts,
        id,
        "createShader",
        &[gl::VERTEX_SHADER],
        &[],
        "",
        None,
    )
    .as_u64()
    .unwrap() as u32;
    command(
        &mut contexts,
        id,
        "bindBuffer",
        &[gl::ARRAY_BUFFER, shader],
        &[],
        "",
        None,
    );
    assert_eq!(
        command(&mut contexts, id, "getError", &[], &[], "", None),
        json!(gl::INVALID_OPERATION)
    );
    contexts.remove(id);
    assert!(contexts.snapshot(id).is_none());
    assert_eq!(contexts.execute(id, "{}", None), json!({"lost":true}));
}
#[test]
fn dropping_peer_context_does_not_invalidate_survivor() {
    let (mut contexts, id) = context();
    let peer = contexts.create(8, 8, "{}").unwrap();
    contexts.remove(peer);
    command(
        &mut contexts,
        id,
        "clearColor",
        &[],
        &[0.0, 1.0, 0.0, 1.0],
        "",
        None,
    );
    command(
        &mut contexts,
        id,
        "clear",
        &[gl::COLOR_BUFFER_BIT],
        &[],
        "",
        None,
    );
    assert_eq!(&contexts.snapshot(id).unwrap().2[..4], &[0, 255, 0, 255]);
    contexts.clear();
}

#[test]
fn presentation_clears_only_unpreserved_buffer_and_restores_clear_state() {
    let (mut contexts, id) = context();
    let preserved = contexts.create(16, 16, "{\"preserve\":true}").unwrap();
    for context in [id, preserved] {
        command(
            &mut contexts,
            context,
            "clearColor",
            &[],
            &[1.0, 0.0, 0.0, 1.0],
            "",
            None,
        );
        command(
            &mut contexts,
            context,
            "clear",
            &[gl::COLOR_BUFFER_BIT],
            &[],
            "",
            None,
        );
        command(&mut contexts, context, "presented", &[], &[], "", None);
    }
    assert_eq!(&contexts.snapshot(id).unwrap().2[..4], &[0, 0, 0, 0]);
    assert_eq!(
        &contexts.snapshot(preserved).unwrap().2[..4],
        &[255, 0, 0, 255]
    );
    command(
        &mut contexts,
        id,
        "clear",
        &[gl::COLOR_BUFFER_BIT],
        &[],
        "",
        None,
    );
    assert_eq!(&contexts.snapshot(id).unwrap().2[..4], &[255, 0, 0, 255]);
}

#[test]
fn peer_resources_cannot_alias_even_when_driver_names_repeat() {
    let (mut contexts, id) = context();
    let peer = contexts.create(8, 8, "{}").unwrap();
    let buffer = command(&mut contexts, id, "createBuffer", &[], &[], "", None)
        .as_u64()
        .unwrap() as u32;
    let other = command(&mut contexts, peer, "createBuffer", &[], &[], "", None)
        .as_u64()
        .unwrap() as u32;
    assert_ne!(buffer, other);
    command(
        &mut contexts,
        peer,
        "bindBuffer",
        &[gl::ARRAY_BUFFER, buffer],
        &[],
        "",
        None,
    );
    assert_eq!(
        command(&mut contexts, peer, "getError", &[], &[], "", None),
        json!(gl::INVALID_OPERATION)
    );
}
