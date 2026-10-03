use super::*;

#[test]
fn worker_offscreen_webgl_executes_on_native_owner_and_exports_real_pixels() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.com/worker.js",
        r#"
            const canvas = new OffscreenCanvas(2,2);
            const gl = canvas.getContext('webgl', {preserveDrawingBuffer:true});
            if (!(gl instanceof WebGLRenderingContext)) throw new Error('WebGL worker context unavailable');
            gl.clearColor(0,0,1,1); gl.clear(gl.COLOR_BUFFER_BIT);
            const bitmap = canvas.transferToImageBitmap();
            const copy = new OffscreenCanvas(2,2).getContext('2d'); copy.drawImage(bitmap,0,0);
            postMessage([...copy.getImageData(0,0,1,1).data].join());
            onmessage = () => {
                const pixels = new Uint8Array(4); gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
                postMessage([...pixels].join());
                close();
            };
        "#,
        "",
        ScriptKind::Classic,
        loader,
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.messages, ["\"0,0,255,255\""]);
    let mut runtime = runtime.unwrap();
    let completion = runtime.dispatch_message("null");
    assert!(completion.errors.is_empty(), "{:?}", completion.errors);
    assert_eq!(completion.messages, ["\"0,0,0,0\""]);
    assert!(completion.closed);
}

#[test]
fn worker_webgl_context_restore_runs_private_tasks_and_exports_restored_pixels() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.com/worker.js",
        r#"
            const canvas=new OffscreenCanvas(2,2), gl=canvas.getContext('webgl',{preserveDrawingBuffer:true});
            const old=gl.createTexture(), lose=gl.getExtension('WEBGL_lose_context');
            canvas.addEventListener('webglcontextlost', event=>{
                if(!event.isTrusted || event.statusMessage!=='')throw Error('worker loss event');
                event.preventDefault();
                // Browser tasks are unaffected by author timer replacements.
                globalThis.setTimeout=()=>{throw Error('author timer intercepted browser task')};
                onmessage=()=>lose.restoreContext();
                postMessage('lost');
            });
            canvas.addEventListener('webglcontextrestored', event=>{
                if(!event.isTrusted || !event.cancelable || gl.isTexture(old))throw Error('worker restore contract');
                gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
                const bitmap=canvas.transferToImageBitmap(), copy=new OffscreenCanvas(2,2).getContext('2d');
                copy.drawImage(bitmap,0,0);postMessage([...copy.getImageData(0,0,1,1).data].join());close();
            });
            lose.loseContext();
            for(let id=1;id<16;id++)clearTimeout(id);
        "#,
        "",
        ScriptKind::Classic,
        loader,
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(initial.messages.is_empty());
    let mut runtime = runtime.unwrap();
    let lost = runtime.advance_time(Duration::from_millis(10), 64);
    assert!(lost.errors.is_empty(), "{:?}", lost.errors);
    assert_eq!(lost.messages, ["\"lost\""]);
    let request = runtime.dispatch_message("null");
    assert!(request.errors.is_empty(), "{:?}", request.errors);
    let restored = runtime.advance_time(Duration::from_millis(10), 64);
    assert!(restored.errors.is_empty(), "{:?}", restored.errors);
    assert_eq!(restored.messages, ["\"0,255,0,255\""]);
    assert!(restored.closed);
}

#[test]
fn worker_webgl_instancing_vertex_arrays_and_shader_extensions_use_real_native_pixels() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.com/worker.js",
        r#"
            const order=[];
            const canvas=new OffscreenCanvas(2,2),gl=canvas.getContext('webgl',{
                get antialias(){order.push('antialias');return true},
                get powerPreference(){order.push('power');return 'low-power'},preserveDrawingBuffer:true});
            const assert=(v,s)=>{if(!v)throw Error(s)};
            assert(order.join()==='antialias,power' && !gl.getContextAttributes().antialias &&
                gl.getContextAttributes().powerPreference==='low-power','worker IDL attributes');
            const instancing=gl.getExtension('ANGLE_instanced_arrays'),arrays=gl.getExtension('OES_vertex_array_object');
            assert(instancing && arrays && gl.getExtension('EXT_frag_depth'),'worker extensions');
            const compile=(kind,source)=>{const s=gl.createShader(kind);gl.shaderSource(s,source);gl.compileShader(s);
                assert(gl.getShaderParameter(s,gl.COMPILE_STATUS),gl.getShaderInfoLog(s));return s;};
            const program=gl.createProgram();
            gl.attachShader(program,compile(gl.VERTEX_SHADER,
                'attribute vec2 p;attribute vec2 offset;void main(){gl_Position=vec4(p+offset,0.,1.);}'));
            gl.attachShader(program,compile(gl.FRAGMENT_SHADER,
                '#extension GL_EXT_frag_depth : require\nprecision mediump float;void main(){gl_FragColor=vec4(0.,0.,1.,1.);gl_FragDepthEXT=.5;}'));
            gl.bindAttribLocation(program,0,'p');gl.bindAttribLocation(program,1,'offset');gl.linkProgram(program);
            assert(gl.getProgramParameter(program,gl.LINK_STATUS),gl.getProgramInfoLog(program));gl.useProgram(program);
            const vao=arrays.createVertexArrayOES();arrays.bindVertexArrayOES(vao);
            gl.bindBuffer(gl.ARRAY_BUFFER,gl.createBuffer());
            gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-1,-1,1,-1,-1,1,1,1]),gl.STATIC_DRAW);
            gl.vertexAttribPointer(0,2,gl.FLOAT,false,0,0);gl.enableVertexAttribArray(0);
            gl.bindBuffer(gl.ARRAY_BUFFER,gl.createBuffer());
            gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([0,0]),gl.STATIC_DRAW);
            gl.vertexAttribPointer(1,2,gl.FLOAT,false,0,0);gl.enableVertexAttribArray(1);
            instancing.vertexAttribDivisorANGLE(1,1);
            arrays.bindVertexArrayOES(null);arrays.bindVertexArrayOES(vao);
            assert(gl.getVertexAttrib(1,instancing.VERTEX_ATTRIB_ARRAY_DIVISOR_ANGLE)===1,'worker VAO state');
            instancing.drawArraysInstancedANGLE(gl.TRIANGLE_STRIP,0,4,1);
            const p=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,p);
            assert(String(p)==='0,0,255,255' && gl.getError()===0,'worker instanced pixels');
            const bitmap=canvas.transferToImageBitmap();
            const copy=new OffscreenCanvas(2,2).getContext('2d');copy.drawImage(bitmap,0,0);
            assert(gl.getSupportedExtensions().length>=7,'worker supported extensions');
            postMessage([...copy.getImageData(0,0,1,1).data].join());
            onmessage=()=>{arrays.deleteVertexArrayOES(vao);gl.getExtension('WEBGL_lose_context').loseContext();close()};
        "#,
        "",
        ScriptKind::Classic,
        loader,
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.messages.len(), 1);
    assert_eq!(initial.messages, ["\"0,0,255,255\""]);
    let mut runtime = runtime.unwrap();
    let shutdown = runtime.dispatch_message("null");
    assert!(shutdown.errors.is_empty(), "{:?}", shutdown.errors);
    assert!(shutdown.closed);
}
