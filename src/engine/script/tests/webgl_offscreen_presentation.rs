//! Offscreen render-target work cannot replace an already displayed Canvas.
use super::*;

fn input(node: &NodeRef, code: &str) -> ScriptInput {
    ScriptInput {
        source_url: "https://example.test/presentation".into(),
        code: code.into(),
        node: node.clone(),
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: false,
    }
}

#[test]
fn fbo_only_clear_does_not_publish_the_implicitly_retired_default_buffer() {
    for api in ["webgl", "webgl2"] {
        for preserve in [false, true] {
            let dom = dom::parse("<canvas width=2 height=2></canvas><script></script>");
            let script = dom.elements_named("script").next().unwrap();
            let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
            let setup = format!(
                r#"
                const canvas=document.querySelector('canvas');
                const gl=canvas.getContext('{api}',{{preserveDrawingBuffer:{preserve},antialias:false}});
                if (!gl) throw Error('real context required');
                gl.clearColor(1,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
                "#
            );
            let outcome = runtime.execute_initial(&[input(&script, &setup)]);
            assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
            let snapshots = runtime.take_canvas_presentation().unwrap();
            assert_eq!(snapshots.len(), 1);
            assert_eq!(
                snapshots[0].pixels.as_ref().unwrap(),
                &[255, 0, 0, 255].repeat(4)
            );

            let pass = r#"
                const fbo=gl.createFramebuffer(), texture=gl.createTexture();
                gl.bindTexture(gl.TEXTURE_2D,texture);
                gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,2,2,0,gl.RGBA,gl.UNSIGNED_BYTE,null);
                gl.bindFramebuffer(gl.FRAMEBUFFER,fbo);
                gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
                gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
                const read=new Uint8Array(4);
                gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,read);
                if([...read].join()!=='0,255,0,255' || gl.getError()) throw Error('offscreen pixels');
            "#;
            let outcome = runtime.execute_additional_with_loader(&[input(&script, pass)], None);
            assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
            assert!(
                runtime.take_canvas_presentation().unwrap().is_empty(),
                "{api}, preserve={preserve}"
            );

            let redraw = "gl.bindFramebuffer(gl.FRAMEBUFFER,null);gl.clearColor(0,0,1,1);gl.clear(gl.COLOR_BUFFER_BIT);";
            let outcome = runtime.execute_additional_with_loader(&[input(&script, redraw)], None);
            assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
            let snapshots = runtime.take_canvas_presentation().unwrap();
            assert_eq!(snapshots.len(), 1);
            assert_eq!(
                snapshots[0].pixels.as_ref().unwrap(),
                &[0, 0, 255, 255].repeat(4)
            );
        }
    }
}
