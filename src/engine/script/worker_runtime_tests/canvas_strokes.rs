use super::*;

#[test]
fn offscreen_fills_union_compound_coverage_before_opacity() {
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/fills.js",
        r#"
        const c=new OffscreenCanvas(32,32).getContext('2d'),p=new Path2D();
        const alpha=(x,y)=>c.getImageData(x,y,1,1).data[3];
        p.rect(.5,0,2,2);p.rect(.5,0,2,2);c.globalAlpha=.5;c.fill(p);
        if(Math.abs(alpha(0,0)-64)>1||Math.abs(alpha(1,0)-128)>1)throw Error('worker fill coverage');
        c.clearRect(0,0,32,32);c.fill(p,'evenodd');
        if(alpha(0,0)||alpha(1,0))throw Error('worker winding rule');
        c.globalAlpha=1;c.setTransform(1,0,0,1,.5,4);c.beginPath();c.rect(4,0,1,1);
        c.resetTransform();c.fill();
        if(Math.abs(alpha(4,4)-128)>1||Math.abs(alpha(5,4)-128)>1)throw Error('worker construction transform');
        postMessage('passed');
        "#,
        "",
        ScriptKind::Classic,
        Arc::new(|url, _| Err(format!("unexpected worker fetch {url}"))),
    );
    assert!(runtime.is_some());
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.messages, ["\"passed\""]);
    assert!(initial.fetch_actions.is_empty());
}

#[test]
fn offscreen_rectangles_preserve_double_geometry_and_whole_pixel_erasure() {
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/rectangles.js",
        r#"
        const c=new OffscreenCanvas(32,32).getContext('2d');
        const alpha=(x,y)=>c.getImageData(x,y,1,1).data[3];
        c.fillRect(.25,.25,1.5,1.5);
        if(Math.abs(alpha(0,0)-143)>1||Math.abs(alpha(1,1)-143)>1)throw Error('worker fractional fill');
        c.clearRect(0,0,32,32);c.fillStyle='red';c.fillRect(0,0,32,32);
        c.globalAlpha=.1;c.globalCompositeOperation='copy';c.clearRect(.5,0,1,1);
        if(alpha(0,0)!==255||alpha(1,0))throw Error('worker fractional clear');
        c.globalCompositeOperation='source-over';c.globalAlpha=1;c.clearRect(0,0,32,32);
        c.setTransform(1,0,1,1,0,0);c.fillRect(0,0,1,1);
        if(Math.abs(alpha(0,0)-128)>1||Math.abs(alpha(1,0)-128)>1)throw Error('worker shear coverage');
        postMessage('passed');
        "#,
        "",
        ScriptKind::Classic,
        Arc::new(|url, _| Err(format!("unexpected worker fetch {url}"))),
    );
    assert!(runtime.is_some());
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.messages, ["\"passed\""]);
    assert!(initial.fetch_actions.is_empty());
}

#[test]
fn offscreen_canvas_uses_the_same_native_dash_caps_joins_and_phase_resets() {
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/strokes.js",
        r#"
        const c=new OffscreenCanvas(64,64).getContext('2d');
        c.lineWidth=4;c.lineCap='round';c.setLineDash([8,8]);
        c.beginPath();c.moveTo(8,20);c.lineTo(18,20);c.moveTo(8,40);c.lineTo(48,40);c.stroke();
        const alpha=(x,y)=>c.getImageData(x,y,1,1).data[3];
        if(alpha(10,20)!==255||alpha(10,40)!==255||!alpha(17,40)||alpha(17,40)===255||alpha(20,40)!==0)
            throw Error('worker dash geometry');
        if(!c.isPointInStroke(17.5,40.5)||c.isPointInStroke(20.5,40.5))throw Error('worker dash hit');
        c.clearRect(0,0,64,64);c.setLineDash([8,24]);c.lineCap='butt';
        const p=new Path2D();p.moveTo(8,20);p.lineTo(24,20);p.lineTo(24,44);c.stroke(p);
        if(alpha(25,19)||c.isPointInStroke(p,25.5,19.5))throw Error('worker gap retained join');
        postMessage('passed');
        "#,
        "",
        ScriptKind::Classic,
        Arc::new(|url, _| Err(format!("unexpected worker fetch {url}"))),
    );
    assert!(runtime.is_some());
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.messages, ["\"passed\""]);
    assert!(initial.fetch_actions.is_empty());
}
