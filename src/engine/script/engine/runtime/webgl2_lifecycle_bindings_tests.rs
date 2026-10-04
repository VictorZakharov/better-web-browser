//! Restoration must create a fresh GLES3 context in both Window and Worker.
use super::webgl2_bindings_tests::{check, document, staged_bootstrap};
use super::*;
use crate::engine::script::{ScriptKind, ScriptOutcome, worker_host::WorkerHostState};
use std::sync::Arc;
use std::time::Duration;

const SETUP: &str = r#"
    const canvas=new OffscreenCanvas(3,3),gl=__stageWebGl2(canvas);
    const events=[],extension=gl.getExtension('WEBGL_lose_context');
    const oldArray=gl.createVertexArray(),oldSampler=gl.createSampler(),oldBuffer=gl.createBuffer();
    const oldFence=gl.fenceSync(gl.SYNC_GPU_COMMANDS_COMPLETE,0);
    gl.bindVertexArray(oldArray);gl.bindSampler(0,oldSampler);
    gl.bindBuffer(gl.UNIFORM_BUFFER,oldBuffer);gl.bufferData(gl.UNIFORM_BUFFER,16,gl.STATIC_DRAW);
    gl.bindBufferBase(gl.UNIFORM_BUFFER,0,oldBuffer);
    gl.pixelStorei(gl.UNPACK_ROW_LENGTH,3);gl.pixelStorei(gl.UNPACK_IMAGE_HEIGHT,4);
    canvas.addEventListener('webglcontextlost',event=>{
        events.push('lost:'+event.isTrusted+':'+event.cancelable);event.preventDefault();
        Promise.resolve().then(()=>events.push('loss-job'));
    });
    canvas.addEventListener('webglcontextrestored',event=>events.push('restored:'+event.isTrusted));
    extension.loseContext();
    if (!gl.isContextLost() || gl.drawingBufferWidth!==0 || events.length!==0)
        throw Error('context loss must be immediate, notification a later task');
    if(gl.getError()!==gl.CONTEXT_LOST_WEBGL || gl.getError()!==0)
        throw Error('context-loss error must be reported once');
"#;

const RESTORE: &str = r#"
    if(events.join()!=='lost:true:true,loss-job') throw Error('loss task and microtask order '+events);
    globalThis.setTimeout=()=>{throw Error('author timer must not schedule restoration');};
    extension.restoreContext();
    if(!gl.isContextLost()) throw Error('restoration cannot occur synchronously');
"#;

const VERIFY: &str = r#"
    if(gl.isContextLost() || events.join()!=='lost:true:true,loss-job,restored:true')
        throw Error('restoration event order '+events);
    if(gl.drawingBufferWidth!==3 || !gl.getParameter(gl.VERSION).startsWith('WebGL 2.0') ||
        !gl.getParameter(gl.SHADING_LANGUAGE_VERSION).startsWith('WebGL GLSL ES 3.00'))
        throw Error('restoration did not recreate GLES3 context');
    if(!gl.getContextAttributes().antialias || gl.getParameter(gl.SAMPLES)!==4)
        throw Error('restoration did not recreate the actual antialiased surface');
    for(const pname of [gl.UNPACK_ROW_LENGTH,gl.UNPACK_IMAGE_HEIGHT])
        if(gl.getParameter(pname)!==0) throw Error('restoration retained unpack state');
    if(gl.getParameter(gl.VERTEX_ARRAY_BINDING)!==null || gl.getIndexedParameter(gl.UNIFORM_BUFFER_BINDING,0)!==null)
        throw Error('restoration retained old container or indexed binding');
    for(const call of [()=>gl.bindVertexArray(oldArray),()=>gl.bindSampler(0,oldSampler),
        ()=>gl.bindBufferBase(gl.UNIFORM_BUFFER,0,oldBuffer),()=>gl.clientWaitSync(oldFence,0,0)]) {
        call();if(gl.getError()!==gl.INVALID_OPERATION) throw Error('old generation resource accepted');
    }
    const array=gl.createVertexArray();gl.bindVertexArray(array);
    if(!gl.isVertexArray(array) || gl.getError()!==0) throw Error('new generation unavailable');
    gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
    const pixel=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
    if([...pixel].join()!=='0,255,0,255' || gl.getError()!==0) throw Error('restored native pixels');
"#;

fn drain_document(context: &mut Context, host: &Rc<RefCell<crate::engine::script::HostState>>) {
    let mut outcome = ScriptOutcome::default();
    for _ in 0..4 {
        crate::engine::script::timer_execution::settle_timer_slice(
            context,
            host,
            &mut outcome,
            &mut None,
            &std::cell::Cell::new(0),
            crate::engine::script::timer_execution::TimerSlice {
                advance: Duration::ZERO,
                max_callbacks: 4,
            },
            None,
        );
    }
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn webgl2_realm_document_restores_driver_version_and_rejects_old_resource_generations() {
    let (mut context, host) = document();
    check(&mut context, SETUP);
    context.run_jobs().unwrap();
    check(
        &mut context,
        "if(events.length!==0) throw Error('jobs dispatched a loss task');",
    );
    drain_document(&mut context, &host);
    check(&mut context, RESTORE);
    drain_document(&mut context, &host);
    check(&mut context, VERIFY);
}

#[test]
fn webgl2_realm_worker_has_identical_native_restoration_and_bitmap_upload_contracts() {
    let host = Rc::new(RefCell::new(WorkerHostState::new(
        "https://example.test/worker.js",
        true,
        "",
        ScriptKind::Classic,
        Arc::new(|url, _| Err(format!("unexpected {url}"))),
        Arc::new(crate::fetch::csp::PolicyContainer::default()),
    )));
    let mut context = Context::new(HostBridge::Worker(Rc::downgrade(&host))).unwrap();
    let source = staged_bootstrap(crate::engine::script::worker_bootstrap::WORKER_BOOTSTRAP);
    check(&mut context, &source);
    check(&mut context, SETUP);
    drain_worker(&mut context, &host);
    check(&mut context, RESTORE);
    drain_worker(&mut context, &host);
    check(&mut context, VERIFY);
    check(
        &mut context,
        r#"
        {
        const image=new ImageData(new Uint8ClampedArray([12,34,56,255]),1,1);
        const texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,texture);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,gl.RGBA,gl.UNSIGNED_BYTE,image);
        const framebuffer=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        const pixel=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        if([...pixel].join()!=='12,34,56,255' || gl.getError()!==0) throw Error('worker DOM bitmap pixels');
        if(typeof WebGL2RenderingContext!=='undefined') throw Error('staged Worker API leaked');
        }
    "#,
    );
}

fn drain_worker(context: &mut Context, host: &Rc<RefCell<WorkerHostState>>) {
    for _ in 0..8 {
        let id = host.borrow_mut().take_ready_timer();
        let Some(id) = id else {
            break;
        };
        context.call_global("__runTimer", &[id.into()]).unwrap();
        context.run_jobs().unwrap();
        context.complete_gpu_task().unwrap();
    }
}
