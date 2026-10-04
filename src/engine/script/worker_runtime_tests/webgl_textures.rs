use super::*;

#[test]
fn worker_webgl_compressed_texture_admission_and_typed_uploads_match_window() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let setup = crate::engine::script::tests::webgl_compressed_textures::SETUP.replace(
        "document.querySelector('canvas').getContext('webgl',{preserveDrawingBuffer:true})",
        "new OffscreenCanvas(4,4).getContext('webgl',{preserveDrawingBuffer:true})",
    );
    let code = format!(
        r#"{setup}
        gl.compressedTexImage2D(gl.TEXTURE_2D,0,0x83f1,4,4,0,red);error(0,'worker block upload');
        gl.compressedTexSubImage2D(gl.TEXTURE_2D,0,0,0,4,4,0x83f1,red);error(0,'worker update');
        for(const name of names){{assert(gl.getExtension(name),name)}}
        assert(gl.getParameter(gl.COMPRESSED_TEXTURE_FORMATS).length===12,'worker format query');
        postMessage('pass');close();
    "#
    );
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.com/worker.js",
        &code,
        "",
        ScriptKind::Classic,
        loader,
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.messages, [r#""pass""#]);
    assert!(initial.closed);
    drop(runtime);
}

#[test]
fn worker_webgl_mrt_shader_writes_four_independent_native_color_targets() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let setup = crate::engine::script::tests::webgl_draw_buffers::SETUP.replace(
        "document.querySelector('canvas').getContext('webgl',{preserveDrawingBuffer:true})",
        "new OffscreenCanvas(2,2).getContext('webgl',{preserveDrawingBuffer:true})",
    );
    let code = format!(
        r#"{setup}
        gl.drawArrays(gl.TRIANGLE_STRIP,0,4);error(0,'worker MRT draw');
        postMessage(textures.map(inspect).join(';'));close();
    "#
    );
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.com/worker.js",
        &code,
        "",
        ScriptKind::Classic,
        loader,
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(
        initial.messages,
        [r#""255,0,0,255;0,255,0,255;0,0,255,255;255,255,255,255""#]
    );
    assert!(initial.closed);
    drop(runtime);
}

#[test]
fn worker_webgl_hdr_binary_readback_preserves_view_bounds_and_shutdown_ownership() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.com/worker.js",
        r#"
        const canvas=new OffscreenCanvas(2,2), gl=canvas.getContext('webgl');
        const assert=(v,s)=>{if(!v)throw Error(s)};
        const half=gl.getExtension('OES_texture_half_float');assert(half,'worker half textures');
        const color=gl.getExtension('EXT_color_buffer_half_float');assert(color,'worker HDR target');
        assert(typeof EXT_color_buffer_half_float==='undefined','worker legacy no interface');
        const rbo=gl.createRenderbuffer();gl.bindRenderbuffer(gl.RENDERBUFFER,rbo);
        gl.renderbufferStorage(gl.RENDERBUFFER,color.RGBA16F_EXT,2,2);
        const fbo=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,fbo);
        gl.framebufferRenderbuffer(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.RENDERBUFFER,rbo);
        assert(gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE,'worker framebuffer');
        gl.clearColor(2,-1,.5,.25);gl.clear(gl.COLOR_BUFFER_BIT);
        const storage=new Float32Array([91,92,93,94,95,96,97,98]);
        gl.readPixels(0,0,1,1,gl.RGBA,gl.FLOAT,storage.subarray(2,6));
        assert(gl.getError()===0,'worker binary float read');
        postMessage([...storage].join());
        onmessage=()=>{
            gl.bindFramebuffer(gl.FRAMEBUFFER,null);
            gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
            const bytes=new Uint8Array([7,8,9,10,11,12,13,14]);
            gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,bytes.subarray(2,6));
            assert(gl.getError()===0,'worker normalized binary read');
            postMessage([...bytes].join());gl.deleteFramebuffer(fbo);gl.deleteRenderbuffer(rbo);close();
        };
        "#,
        "",
        ScriptKind::Classic,
        loader,
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.messages, ["\"91,92,2,-1,0.5,0.25,97,98\""]);
    let mut runtime = runtime.unwrap();
    let response = runtime.dispatch_message("null");
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(response.messages, ["\"7,8,0,255,0,255,13,14\""]);
    assert!(response.closed);
}

#[test]
fn worker_webgl_srgb_renderbuffer_and_depth_extensions_have_real_context_local_storage() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.com/worker.js",
        r#"
        const gl=new OffscreenCanvas(2,2).getContext('webgl');
        const assert=(v,s)=>{if(!v)throw Error(s)};
        const srgb=gl.getExtension('EXT_sRGB'), depth=gl.getExtension('WEBGL_depth_texture');
        assert(srgb&&depth,'worker color/depth capabilities');
        const rbo=gl.createRenderbuffer();gl.bindRenderbuffer(gl.RENDERBUFFER,rbo);
        gl.renderbufferStorage(gl.RENDERBUFFER,srgb.SRGB8_ALPHA8_EXT,2,2);
        const texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,texture);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.DEPTH_COMPONENT,2,2,0,gl.DEPTH_COMPONENT,gl.UNSIGNED_SHORT,null);
        const fbo=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,fbo);
        gl.framebufferRenderbuffer(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.RENDERBUFFER,rbo);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.DEPTH_ATTACHMENT,gl.TEXTURE_2D,texture,0);
        assert(gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE,'worker sRGB with depth');
        assert(gl.getFramebufferAttachmentParameter(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,srgb.FRAMEBUFFER_ATTACHMENT_COLOR_ENCODING_EXT)===srgb.SRGB_EXT,'worker encoded storage');
        gl.clearColor(0,0,0,1);gl.clearDepth(.25);gl.clear(gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT);
        const bytes=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,bytes);
        assert(gl.getError()===0,'worker storage read');postMessage([...bytes].join());
        const peer=new OffscreenCanvas(1,1).getContext('webgl');
        peer.bindTexture(peer.TEXTURE_2D,peer.createTexture());
        peer.texImage2D(peer.TEXTURE_2D,0,gl.DEPTH_COMPONENT,1,1,0,gl.DEPTH_COMPONENT,gl.UNSIGNED_SHORT,null);
        assert(peer.getError()===peer.INVALID_ENUM,'worker peer capability not inherited');close();
        "#,
        "",
        ScriptKind::Classic,
        loader,
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.messages, ["\"0,0,0,255\""]);
    assert!(initial.closed);
    drop(runtime);
}
