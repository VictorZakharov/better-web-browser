//! Compare the retained byte-copy fallback with the native paint transfer.
//! Native tests use WARP; this measures a completed command, not gameplay FPS.
use super::*;
use std::time::Instant;

#[test]
#[ignore = "release command measurements; run alone with --nocapture"]
fn canvas_paint_transfer_command_samples() {
    for (width, height) in [(128, 128), (1382, 760), (2048, 1024)] {
        let dom = dom::parse(&format!(
            "<canvas width={width} height={height}></canvas><script></script>"
        ));
        let script = dom.elements_named("script").next().unwrap();
        let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
        let outcome = runtime.execute_initial(&[input(
            &script,
            &format!(
                r#"
            const canvas=document.querySelector('canvas');
            const gl=canvas.getContext('webgl2',{{antialias:false,preserveDrawingBuffer:true}});
            if(!gl)throw Error('native WebGL context missing');
            function draw(){{
                gl.disable(gl.SCISSOR_TEST);
                gl.clearColor(.25,.125,.0625,.5);gl.clear(gl.COLOR_BUFFER_BIT);
                gl.enable(gl.SCISSOR_TEST);gl.scissor(0,{height}/2,{width},{height}/2);
                gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);gl.finish();
                if(gl.getError())throw Error('draw error');
            }}
        "#
            ),
        )]);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        for round in 0..3 {
            let routes = if round % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            };
            for native in routes {
                for sample in 0..12 {
                    run(&mut runtime, &script, "draw()");
                    let started = Instant::now();
                    let pixels = if native {
                        runtime
                            .take_canvas_presentation()
                            .unwrap()
                            .pop()
                            .unwrap()
                            .pixels
                            .unwrap()
                    } else {
                        // An inactive transfer batch takes the ordinary owned
                        // V8-copy fallback used outside an embedder capture.
                        let JsValue::Array(mut records) = runtime
                            .context
                            .as_deref_mut()
                            .unwrap()
                            .call_global("__takeCanvasPresentation", &[])
                            .unwrap()
                        else {
                            panic!("legacy records missing")
                        };
                        let JsValue::Array(mut fields) = records.pop().unwrap() else {
                            panic!("legacy record missing")
                        };
                        let JsValue::Bytes(pixels) = fields.pop().unwrap() else {
                            panic!("legacy pixel copy missing")
                        };
                        pixels
                    };
                    let elapsed = started.elapsed().as_secs_f64() * 1000.;
                    assert_eq!(pixels.len(), width * height * 4);
                    for (row, bytes) in pixels.chunks_exact(width * 4).enumerate() {
                        let expected = if row < height / 2 {
                            [0, 255, 0, 255]
                        } else {
                            [128, 64, 32, 128]
                        };
                        assert!(
                            bytes.chunks_exact(4).all(|pixel| pixel == expected),
                            "row {row}"
                        );
                    }
                    println!(
                        "width={width};height={height};round={round};native={native};sample={sample};ms={elapsed:.6}"
                    );
                }
            }
        }
    }
}
