use super::*;

#[test]
fn gpu_canvas_resize_reenters_gpu_after_its_geometry_snapshot() {
    let dom = dom::parse("<canvas width=4 height=2></canvas><script></script>");
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    let outcome=runtime.execute_initial(&[input(&script,"const canvas=document.querySelector('canvas');const gl=canvas.getContext('webgl2',{antialias:true});gl.clear(gl.COLOR_BUFFER_BIT);")]);
    assert!(outcome.errors.is_empty());
    runtime.take_canvas_presentation().unwrap();
    runtime.set_gpu_canvas_enabled(true);
    run(
        &mut runtime,
        &script,
        "gl.clearColor(1,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);",
    );
    assert!(
        runtime.take_canvas_presentation().unwrap()[0]
            .gpu_frame
            .is_some()
    );
    run(
        &mut runtime,
        &script,
        "canvas.width=2;canvas.height=4;gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);",
    );
    let frames = runtime.take_canvas_presentation().unwrap();
    assert!(frames[0].pixels.is_some());
    run(&mut runtime, &script, "gl.clear(gl.COLOR_BUFFER_BIT);");
    let frames = runtime.take_canvas_presentation().unwrap();
    let frame = frames[0]
        .gpu_frame
        .expect("resized native pool must be admitted again");
    assert_eq!((frame.width, frame.height), (2, 4));
    run(
        &mut runtime,
        &script,
        "if(gl.getError())throw Error('pool retirement changed author GL errors');",
    );
}

#[test]
fn gpu_canvas_presentation_does_not_enter_active_author_transform_feedback() {
    let dom = dom::parse("<canvas width=4 height=2></canvas><script></script>");
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    let outcome=runtime.execute_initial(&[input(&script,r#"
        const gl=document.querySelector('canvas').getContext('webgl2',{antialias:false,preserveDrawingBuffer:true});
        gl.clearColor(1,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
    "#)]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    runtime.take_canvas_presentation().unwrap();
    runtime.set_gpu_canvas_enabled(true);
    run(
        &mut runtime,
        &script,
        r#"
        function shader(type,source){const s=gl.createShader(type);gl.shaderSource(s,source);gl.compileShader(s);
          if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw Error(gl.getShaderInfoLog(s));return s;}
        const program=gl.createProgram();
        gl.attachShader(program,shader(gl.VERTEX_SHADER,'#version 300 es\nout float value;void main(){value=1.0;gl_Position=vec4(0,0,0,1);}'));
        gl.attachShader(program,shader(gl.FRAGMENT_SHADER,'#version 300 es\nprecision highp float;out vec4 color;void main(){color=vec4(1);}'));
        gl.transformFeedbackVaryings(program,['value'],gl.INTERLEAVED_ATTRIBS);gl.linkProgram(program);
        if(!gl.getProgramParameter(program,gl.LINK_STATUS))throw Error(gl.getProgramInfoLog(program));
        gl.useProgram(program);
        const buffer=gl.createBuffer();gl.bindBuffer(gl.TRANSFORM_FEEDBACK_BUFFER,buffer);
        gl.bufferData(gl.TRANSFORM_FEEDBACK_BUFFER,8,gl.DYNAMIC_READ);gl.bindBufferBase(gl.TRANSFORM_FEEDBACK_BUFFER,0,buffer);
        gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
        const query=gl.createQuery();gl.beginQuery(gl.TRANSFORM_FEEDBACK_PRIMITIVES_WRITTEN,query);
        gl.enable(gl.RASTERIZER_DISCARD);gl.beginTransformFeedback(gl.POINTS);gl.drawArrays(gl.POINTS,0,1);
    "#,
    );
    assert!(
        runtime.take_canvas_presentation().unwrap()[0]
            .gpu_frame
            .is_some()
    );
    run(
        &mut runtime,
        &script,
        r#"
        if(!gl.isEnabled(gl.RASTERIZER_DISCARD))throw Error('rasterizer discard changed');
        gl.drawArrays(gl.POINTS,0,1);gl.endTransformFeedback();gl.endQuery(gl.TRANSFORM_FEEDBACK_PRIMITIVES_WRITTEN);gl.finish();
        const values=new Float32Array(2);gl.getBufferSubData(gl.TRANSFORM_FEEDBACK_BUFFER,0,values);
        if(values.join()!=='1,1')throw Error('private draws entered transform feedback: '+values);
        if(gl.getError())throw Error('presentation left an author GL error');
    "#,
    );
    // GPU query results become visible at the next host task boundary.
    run(
        &mut runtime,
        &script,
        "if(gl.getQueryParameter(query,gl.QUERY_RESULT)!==2)throw Error('author query result changed');",
    );
}

#[test]
fn gpu_canvas_recovery_keeps_the_last_frame_after_unpreserved_clear() {
    for (alpha, premultiplied, expected) in [
        (false, true, [153, 102, 51, 255]),
        (true, true, [128, 102, 51, 128]),
        (true, false, [77, 51, 26, 128]),
    ] {
        let dom = dom::parse("<canvas width=4 height=2></canvas><script></script>");
        let script = dom.elements_named("script").next().unwrap();
        let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
        let outcome=runtime.execute_initial(&[input(&script,&format!(r#"
          const canvas=document.querySelector('canvas');
          const gl=canvas.getContext('webgl2',{{alpha:{alpha},premultipliedAlpha:{premultiplied},antialias:false,preserveDrawingBuffer:false}});
          gl.clearColor(1,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
        "#))]);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert!(
            runtime.take_canvas_presentation().unwrap()[0]
                .pixels
                .is_some()
        );
        runtime.set_gpu_canvas_enabled(true);
        run(
            &mut runtime,
            &script,
            "gl.clearColor(.2,.4,.6,.5);gl.clear(gl.COLOR_BUFFER_BIT);",
        );
        let frames = runtime.take_canvas_presentation().unwrap();
        assert!(frames[0].gpu_frame.is_some());
        assert!(frames[0].pixels.is_none());
        run(
            &mut runtime,
            &script,
            &format!(
                r#"
          const current=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,current);
          if(current.join()!=='0,0,0,{}')throw Error('drawing buffer was not retired independently');
        "#,
                if alpha { 0 } else { 255 }
            ),
        );
        runtime.set_gpu_canvas_enabled(false);
        let mut frames = runtime.take_canvas_presentation().unwrap();
        assert_eq!(frames.len(), 1);
        assert!(frames[0].gpu_frame.is_none());
        let bitmap = crate::engine::image_decode::RasterImage {
            width: 4,
            height: 2,
            rgba: frames.remove(0).pixels.unwrap(),
            rgba16: None,
        }
        .into_premultiplied_bgra();
        assert!(
            bitmap.bgra.chunks_exact(4).all(|pixel| pixel == expected),
            "alpha={alpha}, premultiplied={premultiplied}: {:?}",
            bitmap.bgra
        );
        assert!(
            runtime.take_canvas_presentation().unwrap().is_empty(),
            "repair must retire its GPU marker"
        );
    }
}
