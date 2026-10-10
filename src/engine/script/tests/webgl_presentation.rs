use super::*;
fn input(node: &NodeRef, code: String) -> ScriptInput {
    ScriptInput {
        source_url: "https://example.test/#webgl".into(),
        code,
        node: node.clone(),
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: false,
    }
}
#[test]
fn webgl_native_paint_clears_only_after_presentation_not_export() {
    for preserve in [false, true] {
        let dom = dom::parse_with_scripting(
            &format!(
                r#"
            <canvas width=1 height=1></canvas><script>
                const canvas = document.querySelector('canvas');
                const gl = canvas.getContext('webgl',{{preserveDrawingBuffer:{preserve}}});
                gl.clearColor(1,0,0,1); gl.clear(gl.COLOR_BUFFER_BIT);
                const before = canvas.toDataURL();
                const pixels = new Uint8Array(4); gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
                if ([...pixels].join() !== '255,0,0,255') throw new Error('export discarded GL pixels');
            </script>"#
            ),
            true,
        );
        let script = dom.elements_named("script").next().unwrap();
        let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
        let outcome = runtime.execute_initial(&[input(&script, script.text_content())]);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        let snapshots = runtime.take_canvas_presentation().unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].pixels.as_ref().unwrap(), &[255, 0, 0, 255]);
        assert!(runtime.take_canvas_presentation().unwrap().is_empty());
        let expected = if preserve { "255,0,0,255" } else { "0,0,0,0" };
        let outcome = runtime.execute_additional_with_loader(&[input(&script,format!(
            "gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels); if ([...pixels].join() !== '{expected}') throw new Error('bad presentation retirement');"
        ))],None);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    }
}
#[test]
fn webgl_premultiplied_alpha_is_converted_once_for_canvas_paint() {
    for premultiplied in [false, true] {
        let red = if premultiplied { 0.5 } else { 1.0 };
        let dom = dom::parse_with_scripting(
            &format!(
                r#"
            <canvas width=1 height=1></canvas><script>
                const canvas = document.querySelector('canvas');
                const gl = canvas.getContext('webgl',{{premultipliedAlpha:{premultiplied},preserveDrawingBuffer:true}});
                gl.clearColor({red},0,0,0.5); gl.clear(gl.COLOR_BUFFER_BIT);
            </script>"#
            ),
            true,
        );
        let script = dom.elements_named("script").next().unwrap();
        let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
        let outcome = runtime.execute_initial(&[input(&script, script.text_content())]);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        let snapshots = runtime.take_canvas_presentation().unwrap();
        assert_eq!(snapshots[0].pixels.as_ref().unwrap(), &[255, 0, 0, 128]);
    }
}

#[test]
fn webgl_snapshot_view_uses_private_intrinsics_and_remains_owned_after_later_draws() {
    let dom =
        dom::parse_with_scripting("<canvas width=2 height=2></canvas><script></script>", true);
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    let outcome = runtime.execute_initial(&[input(&script, r#"
        const canvas=document.querySelector('canvas');
        const gl=canvas.getContext('webgl2',{preserveDrawingBuffer:true});
        gl.clearColor(1,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
        const savedConstructor=Uint8ClampedArray,typed=Object.getPrototypeOf(Uint8Array.prototype);
        const saved=[];let snapshotLeaks=0;
        globalThis.Uint8ClampedArray=function(){snapshotLeaks++;throw Error('private pixels leaked');};
        for(const key of ['length','buffer','byteOffset','byteLength']){
            const descriptor=Object.getOwnPropertyDescriptor(typed,key);
            saved[saved.length]=[key,descriptor];
            Object.defineProperty(typed,key,{configurable:true,get(){snapshotLeaks++;throw Error('private bitmap getter leaked');}});
        }
        void 0;
    "#.into())]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let first = runtime.take_canvas_presentation().unwrap();
    assert_eq!(
        first[0].pixels.as_deref().unwrap(),
        [255, 0, 0, 255].repeat(4)
    );
    let outcome = runtime.execute_additional_with_loader(
        &[input(
            &script,
            r#"
        Uint8ClampedArray=savedConstructor;
        for(const [key,descriptor] of saved)Object.defineProperty(typed,key,descriptor);
        if(snapshotLeaks)throw Error('snapshot exposed native storage');
        gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
    "#
            .into(),
        )],
        None,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
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
