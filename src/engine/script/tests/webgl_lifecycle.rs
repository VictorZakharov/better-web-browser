use super::*;
use std::time::Duration;

pub(super) fn run(code: &str) {
    let dom = dom::parse_with_scripting(
        &format!("<canvas width=2 height=2></canvas><script>{code}</script>"),
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let input = ScriptInput {
        source_url: "https://example.test/webgl-loss".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: false,
    };
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    let initial = runtime.execute_initial(&[input]);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    for _ in 0..8 {
        let later = runtime.advance_time(Duration::from_millis(10), 64);
        assert!(later.errors.is_empty(), "{:?}", later.errors);
    }
    assert_eq!(
        dom.elements_named("canvas")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("pass")
    );
}

#[test]
fn webgl_context_loss_and_restore_recreate_native_pixels_and_invalidate_objects() {
    run(r#"
        const canvas=document.querySelector('canvas'), gl=canvas.getContext('webgl',{preserveDrawingBuffer:true});
        const assert=(value,label)=>{if(!value)throw Error(label)};
        const ext=gl.getExtension('WEBGL_lose_context');
        assert(ext===gl.getExtension('webgl_LOSE_context'),'stable case insensitive extension');
        const old=gl.createBuffer(), shader=gl.createShader(gl.VERTEX_SHADER);
        gl.bindBuffer(gl.ARRAY_BUFFER,old);gl.bufferData(gl.ARRAY_BUFFER,16,gl.STATIC_DRAW);
        gl.clearColor(1,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
        gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL,true);
        let events=0;
        canvas.addEventListener('webglcontextlost',event=>{
            events++;
            assert(event.cancelable && !event.bubbles && event.statusMessage==='','loss event contract');
            assert(gl.isContextLost() && gl.drawingBufferWidth===0,'lost buffer');
            assert(gl.getError()===gl.CONTEXT_LOST_WEBGL && gl.getError()===gl.NO_ERROR,'single loss error');
            assert(gl.getContextAttributes()===null && gl.getSupportedExtensions()===null,'lost queries');
            assert(gl.getParameter(gl.UNPACK_FLIP_Y_WEBGL)===null,'lost private state query');
            assert(gl.createBuffer()===null && !gl.isBuffer(old),'lost objects');
            const untouched=new Uint8Array([7,7,7,7]);
            gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,untouched);
            assert([...untouched].join()==='7,7,7,7','lost read does not write');
            event.preventDefault();
            setTimeout(()=>{
                ext.restoreContext();
                assert(gl.isContextLost(),'restore is asynchronous');
            },0);
        });
        canvas.addEventListener('webglcontextrestored',event=>{
            assert(event.cancelable && event.statusMessage==='','restore event contract');
            assert(events===1 && !gl.isContextLost(),'restored');
            assert(canvas.getContext('webgl')===gl,'same JS context');
            assert(!gl.isBuffer(old) && !gl.isShader(shader),'old handles invalidated');
            assert(gl.getError()===gl.NO_ERROR,'restoration clears errors');
            gl.bindBuffer(gl.ARRAY_BUFFER,old);
            assert(gl.getError()===gl.INVALID_OPERATION,'cannot bind old buffer');
            assert(gl.getParameter(gl.ARRAY_BUFFER_BINDING)===null,'binding reset');
            assert(gl.getParameter(gl.UNPACK_FLIP_Y_WEBGL)===false,'unpack reset');
            const pixel=new Uint8Array(4);
            gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
            assert([...pixel].join()==='0,0,0,0','native buffer reset');
            gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
            gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
            assert([...pixel].join()==='0,255,0,255','native rendering after restoration');
            canvas.dataset.result='pass';
        });
        ext.loseContext();
        assert(gl.isContextLost() && events===0,'loss state immediate, event asynchronous');
    "#);
}

#[test]
fn webgl_uncanceled_loss_cannot_restore_and_duplicate_loss_reports_error() {
    run(r#"
        const canvas=document.querySelector('canvas'), gl=canvas.getContext('webgl');
        const ext=gl.getExtension('WEBGL_lose_context');
        const assert=(value,label)=>{if(!value)throw Error(label)};
        ext.restoreContext(); assert(gl.getError()===gl.INVALID_OPERATION,'restore healthy');
        canvas.addEventListener('webglcontextlost',()=>{
            assert(gl.getError()===gl.CONTEXT_LOST_WEBGL,'initial error');
            setTimeout(()=>{
                ext.restoreContext();assert(gl.getError()===gl.INVALID_OPERATION,'uncanceled restore');
                ext.loseContext();assert(gl.getError()===gl.INVALID_OPERATION,'duplicate loss');
                assert(gl.isContextLost(),'still lost');canvas.dataset.result='pass';
            },0);
        });
        ext.loseContext();
    "#);
}

#[test]
fn webgl_loss_event_must_complete_before_restore_request() {
    run(r#"
        const canvas=document.querySelector('canvas'),gl=canvas.getContext('webgl');
        const ext=gl.getExtension('WEBGL_lose_context');
        canvas.addEventListener('webglcontextlost',event=>{
            event.preventDefault();
            ext.restoreContext();
            if(gl.getError()!==gl.CONTEXT_LOST_WEBGL || gl.getError()!==gl.INVALID_OPERATION)
                throw Error('restoration before loss event completion');
            setTimeout(()=>ext.restoreContext(),0);
        });
        canvas.addEventListener('webglcontextrestored',()=>canvas.dataset.result='pass');
        ext.loseContext();
    "#);
}

#[test]
fn webgl_restore_admission_failure_stays_lost_and_can_retry_after_releasing_peer() {
    run(r#"
        const canvas=document.querySelector('canvas'),gl=canvas.getContext('webgl');
        const loss=gl.getExtension('WEBGL_lose_context');let restored=0;
        canvas.addEventListener('webglcontextlost',event=>{
            event.preventDefault();
            const peers=[];
            for(let i=0;i<8;i++){
                const peer=new OffscreenCanvas(1,1).getContext('webgl');
                if(!peer)throw Error('loss did not release native admission');peers.push(peer);
            }
            setTimeout(()=>{
                loss.restoreContext();
                setTimeout(()=>{
                    if(!gl.isContextLost() || restored!==0 || gl.getContextAttributes()!==null)
                        throw Error('failed admission exposed partial restoration');
                    peers[0].getExtension('WEBGL_lose_context').loseContext();
                    loss.restoreContext();
                },0);
            },0);
        });
        canvas.addEventListener('webglcontextrestored',()=>{
            if(++restored!==1 || gl.isContextLost() || gl.getError()!==0)throw Error('retry restoration');
            gl.clearColor(1,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
            const p=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,p);
            if(String(p)!=='255,0,0,255')throw Error('retry did not create native buffer');
            canvas.dataset.result='pass';
        });
        loss.loseContext();
    "#);
}

#[test]
fn webgl_repeated_restore_uses_fresh_epochs_and_resets_drawing_and_error_state() {
    run(r#"
        const canvas=document.querySelector('canvas'),gl=canvas.getContext('webgl',{preserveDrawingBuffer:true});
        const loss=gl.getExtension('WEBGL_lose_context');let cycles=0;
        const retired=[];
        const begin=()=>{
            retired.push(gl.createBuffer());gl.clearColor(1,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
            gl.bindBuffer(0xdead,null);loss.loseContext();
        };
        canvas.addEventListener('webglcontextlost',event=>{
            event.preventDefault();
            if(gl.getError()!==gl.CONTEXT_LOST_WEBGL || gl.getError()!==0)throw Error('old errors survived loss');
            setTimeout(()=>{loss.restoreContext();loss.restoreContext()},0);
        });
        canvas.addEventListener('webglcontextrestored',()=>{
            const p=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,p);
            if(String(p)!=='0,0,0,0' || gl.getParameter(gl.CURRENT_PROGRAM)!==null || gl.getError()!==0)
                throw Error('state survived native recreation');
            for(const b of retired){
                if(gl.isBuffer(b))throw Error('old epoch became live');
                gl.bindBuffer(gl.ARRAY_BUFFER,b);if(gl.getError()!==gl.INVALID_OPERATION)throw Error('old binding accepted');
            }
            if(++cycles===3)canvas.dataset.result='pass';else setTimeout(begin,0);
        });
        begin();
    "#);
}

#[test]
fn webgl_restore_uses_current_canvas_size_without_converting_new_context_options() {
    run(r#"
        const canvas=document.querySelector('canvas'),gl=canvas.getContext('webgl');
        const loss=gl.getExtension('WEBGL_lose_context');
        canvas.addEventListener('webglcontextlost',event=>{
            event.preventDefault();canvas.width=4;canvas.height=3;
            const ignored=new Proxy({}, {get(){throw Error('lost context option conversion')}});
            if(canvas.getContext('webgl',ignored)!==gl)throw Error('lost JS context changed');
            setTimeout(()=>loss.restoreContext(),0);
        });
        canvas.addEventListener('webglcontextrestored',()=>{
            if(gl.drawingBufferWidth!==4 || gl.drawingBufferHeight!==3)throw Error('restored stale surface');
            if(String(gl.getParameter(gl.VIEWPORT))!=='0,0,4,3')throw Error('restored viewport');
            gl.clearColor(0,0,1,1);gl.clear(gl.COLOR_BUFFER_BIT);
            const p=new Uint8Array(4);gl.readPixels(3,2,1,1,gl.RGBA,gl.UNSIGNED_BYTE,p);
            if(String(p)!=='0,0,255,255')throw Error('resized native surface');
            canvas.dataset.result='pass';
        });
        loss.loseContext();
    "#);
}
