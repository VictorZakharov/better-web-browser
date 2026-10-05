use super::*;

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
