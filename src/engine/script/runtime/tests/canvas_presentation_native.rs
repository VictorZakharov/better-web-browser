//! Runtime paint transfers leave public bitmap snapshots and GL observations independent.
use super::*;
use crate::engine::dom::NodeRef;
use crate::engine::script::engine::JsValue;
use crate::engine::script::{ScriptFetchOptions, ScriptKind};
mod async_publication;
mod command_samples;
mod gpu_recovery;

fn input(node: &NodeRef, code: &str) -> ScriptInput {
    ScriptInput {
        source_url: "https://example.test/native-paint".into(),
        code: code.into(),
        node: node.clone(),
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: false,
    }
}

fn run(runtime: &mut ScriptRuntime, node: &NodeRef, code: &str) {
    let outcome = runtime.execute_additional_with_loader(&[input(node, code)], None);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn native_paint_hook_returns_metadata_and_moves_pixels_outside_v8() {
    for api in ["webgl", "webgl2"] {
        let dom = dom::parse("<canvas width=2 height=2></canvas><script></script>");
        let script = dom.elements_named("script").next().unwrap();
        let canvas = dom.elements_named("canvas").next().unwrap();
        let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
        let outcome = runtime.execute_initial(&[input(
            &script,
            &format!(
                r#"
            const canvas=document.querySelector('canvas');
            const gl=canvas.getContext('{api}',{{antialias:false,preserveDrawingBuffer:true}});
            gl.clearColor(1,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
            if(typeof __takeCanvasPresentation!=='undefined'||typeof __hostCall!=='undefined')
                throw Error('private capture exposed');
        "#
            ),
        )]);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert!(runtime.host.borrow_mut().canvas_presentations.begin());
        let reply = runtime
            .context
            .as_deref_mut()
            .unwrap()
            .call_global("__takeCanvasPresentation", &[])
            .unwrap();
        let mut batch = runtime.host.borrow_mut().canvas_presentations.finish();
        let JsValue::Array(records) = reply else {
            panic!("missing records")
        };
        let JsValue::Array(fields) = &records[0] else {
            panic!("missing fields")
        };
        assert_eq!(fields.len(), 6);
        let token = fields[5]
            .as_number()
            .expect("paint hook must return a scalar token") as u32;
        let pixels = batch.take(token, canvas.id(), 2, 2).unwrap();
        assert_eq!(pixels, [255, 0, 0, 255].repeat(4));
        run(
            &mut runtime,
            &script,
            r#"
            const copy=document.createElement('canvas');copy.width=copy.height=2;
            const ctx=copy.getContext('2d');ctx.drawImage(canvas,0,0);
            if(ctx.getImageData(0,0,1,1).data.join()!=='255,0,0,255')throw Error('public source after paint');
            gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
            if(ctx.getImageData(0,0,1,1).data.join()!=='255,0,0,255')throw Error('public copy changed');
        "#,
        );
        let next = runtime.take_canvas_presentation().unwrap();
        assert_eq!(
            next[0].pixels.as_deref().unwrap(),
            [0, 255, 0, 255].repeat(4)
        );
        assert_eq!(pixels, [255, 0, 0, 255].repeat(4));
    }
}

#[test]
fn native_paint_preserves_public_exports_resize_removal_and_context_loss() {
    for preserve in [false, true] {
        let dom = dom::parse("<canvas width=3 height=2></canvas><script></script>");
        let script = dom.elements_named("script").next().unwrap();
        let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
        let outcome = runtime.execute_initial(&[input(&script, &format!(r#"
            const canvas=document.querySelector('canvas');
            const gl=canvas.getContext('webgl2',{{antialias:false,preserveDrawingBuffer:{preserve}}});
            gl.clearColor(0.25,0.125,0,0.5);gl.clear(gl.COLOR_BUFFER_BIT);
            const raw=new Uint8Array(4);
        "#))]);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        let first = runtime.take_canvas_presentation().unwrap();
        assert_eq!((first[0].width, first[0].height), (3, 2));
        assert_eq!(
            first[0].pixels.as_deref().unwrap(),
            [128, 64, 0, 128].repeat(6)
        );
        let expected = if preserve { "64,32,0,128" } else { "0,0,0,0" };
        run(
            &mut runtime,
            &script,
            &format!(
                r#"
            gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,raw);
            if(raw.join()!=='{expected}')throw Error('retirement changed raw observations');
            canvas.width=1;canvas.height=3;
        "#
            ),
        );
        let resized = runtime.take_canvas_presentation().unwrap();
        assert_eq!((resized[0].width, resized[0].height), (1, 3));
        assert_eq!(resized[0].content_size, (1, 3));
        assert_eq!(resized[0].pixels.as_deref().unwrap(), [0; 12]);
        run(
            &mut runtime,
            &script,
            "canvas.remove();gl.clearColor(0,0,1,1);gl.clear(gl.COLOR_BUFFER_BIT);",
        );
        assert!(runtime.take_canvas_presentation().unwrap().is_empty());
        run(&mut runtime, &script, "document.body.appendChild(canvas);");
        let reattached = runtime.take_canvas_presentation().unwrap();
        assert_eq!(
            reattached[0].pixels.as_deref().unwrap(),
            [0, 0, 255, 255].repeat(3)
        );
        run(
            &mut runtime,
            &script,
            "gl.getExtension('WEBGL_lose_context').loseContext();",
        );
        let lost = runtime.take_canvas_presentation().unwrap();
        assert!(
            lost.is_empty()
                || lost[0]
                    .pixels
                    .as_ref()
                    .is_none_or(|bytes| bytes.iter().all(|byte| *byte == 0))
        );
        assert_eq!(
            first[0].pixels.as_deref().unwrap(),
            [128, 64, 0, 128].repeat(6)
        );
    }
}

#[test]
fn placeholder_native_paint_retires_only_the_frame_and_bitmap_export_stays_owned() {
    let dom = dom::parse("<canvas width=2 height=2></canvas><script></script>");
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    let outcome = runtime.execute_initial(&[input(
        &script,
        r#"
        const canvas=document.querySelector('canvas'),offscreen=canvas.transferControlToOffscreen();
        const gl=offscreen.getContext('webgl2',{antialias:false,preserveDrawingBuffer:true});
        gl.clearColor(1,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
    "#,
    )]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let first = runtime.take_canvas_presentation().unwrap();
    assert_eq!(
        first[0].pixels.as_deref().unwrap(),
        [255, 0, 0, 255].repeat(4)
    );
    run(
        &mut runtime,
        &script,
        r#"
        const bitmap=offscreen.transferToImageBitmap();
        gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
        const copy=new OffscreenCanvas(2,2),ctx=copy.getContext('2d');ctx.drawImage(bitmap,0,0);
        if(ctx.getImageData(0,0,1,1).data.join()!=='255,0,0,255')throw Error('bitmap was aliased');
    "#,
    );
    let second = runtime.take_canvas_presentation().unwrap();
    assert_eq!(
        second[0].pixels.as_deref().unwrap(),
        [0, 255, 0, 255].repeat(4)
    );
    assert_eq!(
        first[0].pixels.as_deref().unwrap(),
        [255, 0, 0, 255].repeat(4)
    );
}

#[test]
fn deferred_native_frame_keeps_unpreserved_pixels_and_gets_first_export_priority() {
    let dom = dom::parse(&format!(
        "{}<canvas id=g width=2 height=2></canvas><script></script>",
        "<canvas width=2048 height=2048></canvas>".repeat(4)
    ));
    let script = dom.elements_named("script").next().unwrap();
    let native_node = dom.elements_named("canvas").nth(4).unwrap().id();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    let outcome = runtime.execute_initial(&[input(
        &script,
        r#"
        const canvases=[...document.querySelectorAll('canvas')];
        for(const canvas of canvases.slice(0,4))canvas.getContext('2d').fillRect(0,0,1,1);
        const gl=canvases[4].getContext('webgl2',{antialias:false,preserveDrawingBuffer:false});
        gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
    "#,
    )]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let first = runtime.take_canvas_presentation().unwrap();
    assert_eq!(first.len(), 4);
    assert!(first.iter().all(|snapshot| snapshot.node != native_node));
    drop(first);
    run(
        &mut runtime,
        &script,
        r#"
        const pixels=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
        if(pixels.join()!=='0,255,0,255')throw Error('deferred frame retired');
        for(const canvas of canvases.slice(0,4))canvas.getContext('2d').fillRect(0,0,1,1);
    "#,
    );
    let second = runtime.take_canvas_presentation().unwrap();
    assert_eq!(second[0].node, native_node);
    assert_eq!(
        second[0].pixels.as_deref().unwrap(),
        [0, 255, 0, 255].repeat(4)
    );
    assert_eq!(second.len(), 4);
    drop(second);
    run(
        &mut runtime,
        &script,
        r#"
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
        if(pixels.join()!=='0,0,0,0')throw Error('accepted frame not retired');
    "#,
    );
}

#[test]
fn earlier_public_bitmap_export_keeps_the_owned_copy_fallback() {
    let dom = dom::parse("<canvas width=2 height=1></canvas><script></script>");
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    let outcome = runtime.execute_initial(&[input(
        &script,
        r#"
        const canvas=document.querySelector('canvas');
        const gl=canvas.getContext('webgl2',{antialias:false,preserveDrawingBuffer:true});
        gl.clearColor(1,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
        const copy=new OffscreenCanvas(2,1),ctx=copy.getContext('2d');ctx.drawImage(canvas,0,0);
        if(ctx.getImageData(0,0,1,1).data.join()!=='255,0,0,255')throw Error('public export');
    "#,
    )]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(runtime.host.borrow_mut().canvas_presentations.begin());
    let JsValue::Array(records) = runtime
        .context
        .as_deref_mut()
        .unwrap()
        .call_global("__takeCanvasPresentation", &[])
        .unwrap()
    else {
        panic!("missing records")
    };
    let JsValue::Array(fields) = &records[0] else {
        panic!("missing record")
    };
    let JsValue::Bytes(pixels) = &fields[5] else {
        panic!("cached export must retain its independent copy")
    };
    assert_eq!(pixels, &[255, 0, 0, 255].repeat(2));
    drop(runtime.host.borrow_mut().canvas_presentations.finish());
    run(
        &mut runtime,
        &script,
        r#"
        gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
        if(ctx.getImageData(0,0,1,1).data.join()!=='255,0,0,255')throw Error('old copy aliased');
    "#,
    );
    assert_eq!(pixels, &[255, 0, 0, 255].repeat(2));
}

#[test]
fn private_native_metadata_does_not_invoke_author_array_setters_or_json_parsers() {
    let dom = dom::parse("<canvas width=2 height=2></canvas><script></script>");
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    let outcome = runtime.execute_initial(&[input(
        &script,
        r#"
        const canvas=document.querySelector('canvas');
        const gl=canvas.getContext('webgl2',{antialias:false,preserveDrawingBuffer:true});
        gl.clearColor(1,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);gl.finish();
        const oldParse=JSON.parse;
        let arrayWrites=0,parseCalls=0;
        Object.defineProperty(Array.prototype,'0',{configurable:true,set(){arrayWrites++;}});
        JSON.parse=()=>{parseCalls++;throw Error('private parse exposed');};
    "#,
    )]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let snapshots = runtime.take_canvas_presentation().unwrap();
    assert_eq!(
        snapshots[0].pixels.as_deref().unwrap(),
        [255, 0, 0, 255].repeat(4)
    );
    run(
        &mut runtime,
        &script,
        r#"
        delete Array.prototype[0];JSON.parse=oldParse;
        if(arrayWrites||parseCalls)throw Error('author hook ran during private transfer');
    "#,
    );
}
