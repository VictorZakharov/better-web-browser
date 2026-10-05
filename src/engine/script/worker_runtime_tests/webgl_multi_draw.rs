use super::*;

#[test]
fn offscreen_multi_draw_has_real_ids_and_atomic_invalid_batches() {
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/multi-draw.js",
        r#"
        const canvas=new OffscreenCanvas(4,4),gl=canvas.getContext('webgl2',{antialias:false});
        const ext=gl.getExtension('WEBGL_multi_draw');if(!ext)throw Error('worker extension');
        const compile=(type,source)=>{const s=gl.createShader(type);gl.shaderSource(s,source);gl.compileShader(s);
            if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw Error(gl.getShaderInfoLog(s));return s;};
        const p=gl.createProgram();gl.attachShader(p,compile(gl.VERTEX_SHADER,
            '#version 300 es\n#extension GL_ANGLE_multi_draw : require\nflat out int draw;void main(){draw=gl_DrawID;gl_Position=vec4(float(draw)*0.5-0.75,0.25,0,1);gl_PointSize=1.0;}'));
        gl.attachShader(p,compile(gl.FRAGMENT_SHADER,
            '#version 300 es\nprecision highp float;flat in int draw;out vec4 color;void main(){color=draw==0?vec4(1,0,0,1):vec4(0,1,0,1);}'));
        gl.linkProgram(p);if(!gl.getProgramParameter(p,gl.LINK_STATUS))throw Error(gl.getProgramInfoLog(p));gl.useProgram(p);
        const pixel=x=>{const d=new Uint8Array(4);gl.readPixels(x,2,1,1,gl.RGBA,gl.UNSIGNED_BYTE,d);return Array.from(d).join(',');};
        ext.multiDrawArraysInstancedWEBGL(gl.POINTS,[0,0],0,[1,1],0,[0,1],0,2);
        if(pixel(0)!=='0,0,0,0'||pixel(1)!=='0,255,0,255')throw Error('worker native draw ID');
        gl.drawArrays(gl.POINTS,0,1);if(pixel(0)!=='255,0,0,255')throw Error('worker ordinary ID');
        gl.clear(gl.COLOR_BUFFER_BIT);
        ext.multiDrawArraysWEBGL(gl.POINTS,[0,-1],0,[1,1],0,2);
        if(gl.getError()!==gl.INVALID_VALUE||pixel(0)!=='0,0,0,0')throw Error('worker atomic validation');
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
