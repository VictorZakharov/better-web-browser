use super::*;
use base64::Engine;

const AHEM: &[u8] = include_bytes!("../../../../tests/canvas/fonts/ahem.ttf");

#[test]
fn worker_font_unicode_ranges_control_real_pixels_and_queries() {
    let encoded = base64::engine::general_purpose::STANDARD.encode(AHEM);
    let source = include_str!("../../../../tests/canvas/font-unicode-ranges.js");
    let (_, initial) = start(&format!(
        "{source}\nconst bytes=Uint8Array.from(atob('{encoded}'),c=>c.charCodeAt(0));testFontUnicodeRanges(bytes).then(postMessage,error=>postMessage('failed:'+error));"
    ));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.messages, ["\"passed\""]);
}

#[test]
fn worker_font_loading_events_use_private_readonly_idl_sequences() {
    let source = include_str!("../../../../tests/canvas/font-face-events.js");
    let (_, initial) = start(&format!("{source}\npostMessage(testFontFaceEvents());"));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.messages, ["\"passed\""]);
}

#[test]
fn worker_font_face_set_uses_the_same_css_query_matching() {
    let encoded = base64::engine::general_purpose::STANDARD.encode(AHEM);
    let matching = include_str!("../../../../tests/canvas/font-face-matching.js");
    let (_, initial) = start(&format!(
        "{matching}\nconst bytes=Uint8Array.from(atob('{encoded}'),c=>c.charCodeAt(0));testFontFaceMatching(bytes).then(postMessage,error=>postMessage('failed:'+error));"
    ));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.messages, ["\"passed\""]);
}

#[test]
fn worker_font_face_uses_the_same_buffer_and_dictionary_binding_contract() {
    let encoded = base64::engine::general_purpose::STANDARD.encode(AHEM);
    let bindings = include_str!("../../../../tests/canvas/font-face-bindings.js");
    let (_, initial) = start(&format!(
        "{bindings}\nconst bytes=Uint8Array.from(atob('{encoded}'),c=>c.charCodeAt(0));testFontFaceBindings(bytes).then(postMessage,error=>postMessage('failed:'+error));"
    ));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.messages, ["\"passed\""]);
}

fn start(source: &str) -> (Option<WorkerRuntime>, WorkerRuntimeOutcome) {
    let (mut runtime, mut outcome) = WorkerRuntime::start(
        "https://example.test/font-worker.js",
        source,
        "",
        ScriptKind::Classic,
        Arc::new(|url, _| Err(format!("unexpected worker fetch {url}"))),
    );
    // Font parsing/completion is task-based. This helper explicitly drives the
    // worker event loop rather than assuming all async work is a microtask.
    if let Some(runtime) = runtime.as_mut() {
        for _ in 0..128 {
            if runtime.next_timer_delay() != Some(Duration::ZERO) {
                break;
            }
            let turn = runtime.advance_time(Duration::ZERO, 1);
            outcome.errors.extend(turn.errors);
            outcome.messages.extend(turn.messages);
            if !outcome.errors.is_empty() {
                break;
            }
        }
    }
    (runtime, outcome)
}

#[test]
fn worker_fonts_shape_and_rasterize_real_bytes_without_inheriting_other_realms() {
    let encoded = base64::engine::general_purpose::STANDARD.encode(AHEM);
    let source = format!(
        r#"
        const assert=(v,m)=>{{if(!v)throw Error(m);}};
        assert(typeof document==='undefined','no worker document');
        assert(fonts===self.fonts&&fonts.size===0,'worker FontFaceSource starts empty');
        const c=new OffscreenCanvas(80,60).getContext('2d');
        c.font='20px WorkerOwnedAhem';
        const fallback=c.measureText('ABC').width;
        const face=new FontFace('WorkerOwnedAhem',Uint8Array.from(atob('{encoded}'),c=>c.charCodeAt(0)));
        face.load().then(async()=>{{
            assert(face.status==='loaded','valid font decoded');
            assert(c.measureText('ABC').width===fallback,'unregistered face has no effect');
            fonts.add(face);
            assert(fonts.has(face)&&fonts.size===1,'registered worker face');
            assert(c.measureText('ABC').width===60,'real Ahem advances');
            c.fillStyle='red';c.fillText('A',10,40);
            const pixels=c.getImageData(0,0,80,60).data;
            let count=0;for(let i=3;i<pixels.length;i+=4)if(pixels[i])count++;
            assert(count===400,'real worker glyph bitmap');
            assert(await fonts.ready===fonts,'worker ready');
            assert(fonts.delete(face),'delete registered face');
            assert(c.measureText('ABC').width===fallback,'delete invalidates worker font selection');
            fonts.add(face);fonts.clear();
            assert(c.measureText('ABC').width===fallback,'clear invalidates worker font selection');
            postMessage('passed');
        }}).catch(e=>postMessage('failed:'+e));
    "#
    );
    let (runtime, initial) = start(&source);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(runtime.is_some());
    assert_eq!(initial.messages, ["\"passed\""]);
    let (_, fresh) = start(
        r#"
        const c=new OffscreenCanvas(80,60).getContext('2d');c.font='20px WorkerOwnedAhem';
        if(fonts.size!==0||Math.abs(c.measureText('ABC').width-60)<0.1)throw Error('foreign worker font leaked');
        postMessage('fresh');
    "#,
    );
    assert!(fresh.errors.is_empty(), "{:?}", fresh.errors);
    assert_eq!(fresh.messages, ["\"fresh\""]);
}

#[test]
fn worker_font_registration_budget_failure_is_atomic_and_deletion_frees_capacity() {
    let encoded = base64::engine::general_purpose::STANDARD.encode(AHEM);
    let (_, initial) = start(&format!(
        r#"
        (async()=>{{
            const assert=(v,m)=>{{if(!v)throw Error(m);}};
            const bytes=Uint8Array.from(atob('{encoded}'),c=>c.charCodeAt(0));
            const faces=[];
            for(let i=0;i<17;i++){{const face=new FontFace('WorkerBudget'+i,bytes);await face.load();faces.push(face);}}
            for(let i=0;i<16;i++)fonts.add(faces[i]);
            let rejected=false;try{{fonts.add(faces[16]);}}catch(e){{rejected=e.name==='SyntaxError';}}
            assert(rejected,'bounded native registry rejects excess admission');
            assert(fonts.size===16&&!fonts.has(faces[16]),'failed admission leaves set unchanged');
            assert(faces[16].status==='loaded','valid face remains loaded');
            fonts.delete(faces[0]);fonts.add(faces[16]);
            const c=new OffscreenCanvas(80,60).getContext('2d');c.font='20px WorkerBudget16';
            assert(fonts.size===16&&c.measureText('ABC').width===60,'deletion frees registry capacity');
            postMessage('passed');
        }})().catch(e=>postMessage('failed:'+e));
    "#
    ));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.messages, ["\"passed\""]);
}
