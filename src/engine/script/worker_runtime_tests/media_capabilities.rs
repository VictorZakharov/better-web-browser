//! Isolated worker queries reuse the actual codec matrix without device grants.

use super::*;

fn start(source: &str) -> (WorkerRuntime, WorkerRuntimeOutcome) {
    let (runtime, outcome) = WorkerRuntime::start(
        "https://example.test/worker.js",
        source,
        "",
        ScriptKind::Classic,
        Arc::new(|url, _| Err(format!("unexpected import {url}"))),
    );
    check(&outcome);
    (runtime.expect("capability worker"), outcome)
}

fn check(outcome: &WorkerRuntimeOutcome) {
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(
        outcome.fetch_actions.is_empty(),
        "pure capability query cannot fetch"
    );
    assert!(
        outcome.database_actions.is_empty(),
        "pure query cannot persist or request trust"
    );
    assert!(outcome.websocket_actions.is_empty());
    assert!(outcome.console.is_empty(), "{:?}", outcome.console);
}

fn verify(source: &str) {
    let (mut runtime, initial) = start(source);
    let mut messages = initial.messages;
    for _ in 0..32 {
        if runtime.next_timer_delay().is_none() {
            break;
        }
        let outcome = runtime.advance_time(Duration::ZERO, 64);
        check(&outcome);
        messages.extend(outcome.messages);
    }
    assert_eq!(messages, ["\"passed\""]);
    assert_eq!(runtime.next_timer_delay(), None);
}

#[test]
fn worker_capability_interface_is_branded_same_object_and_not_a_capture_api() {
    verify(
        r#"
        const cap=navigator.mediaCapabilities;
        if(typeof MediaCapabilities!=='function' || !(cap instanceof MediaCapabilities) ||
            cap!==navigator.mediaCapabilities || Object.getPrototypeOf(cap)!==MediaCapabilities.prototype ||
            Object.prototype.toString.call(cap)!=='[object MediaCapabilities]' ||
            'MediaRecorder' in self || 'mediaDevices' in navigator ||
            '__mediaCapabilitiesQueue' in self) throw Error('worker exposure');
        try{new MediaCapabilities();throw Error('constructor');}
        catch(error){if(error.name!=='TypeError')throw error;}
        const invalid=Object.create(MediaCapabilities.prototype);
        Promise.all(['encodingInfo','decodingInfo'].map(name=>cap[name].call(invalid,{}).then(
            ()=>false,error=>error.name==='TypeError'))).then(results=>{
            if(!results.every(Boolean))throw Error('worker brand'); postMessage('passed');
        });
    "#,
    );
}

#[test]
fn worker_recording_and_file_support_match_real_encoders_and_decoders() {
    verify(
        r#"
        const cap=navigator.mediaCapabilities;
        const cases=[
            ['audio/webm;codecs=opus',true,true],['audio/ogg;codecs=opus',true,true],
            ['audio/flac',true,true],['audio/webm;codecs=vorbis',false,true],
            ['audio/ogg;codecs=vorbis',false,true],['audio/mpeg',false,true],
            ['audio/aac',false,true],['audio/unimplemented',false,false]
        ];
        Promise.all(cases.map(async ([contentType,record,file])=>{
            const audio={contentType,channels:'2',samplerate:48000,bitrate:64000};
            const encoded=await cap.encodingInfo({type:'record',audio});
            const decoded=await cap.decodingInfo({type:'file',audio});
            if(encoded.supported!==record || decoded.supported!==file ||
                encoded.smooth || encoded.powerEfficient || decoded.smooth || decoded.powerEfficient ||
                'keySystemAccess' in encoded || decoded.keySystemAccess!==null)
                throw Error('worker support '+contentType);
        })).then(()=>postMessage('passed'));
    "#,
    );
}

#[test]
fn worker_opus_encoding_shape_matches_capture_bounds_not_decoder_rates() {
    verify(
        r#"
        const cap=navigator.mediaCapabilities;
        const base={contentType:'audio/webm;codecs=opus',channels:'2',samplerate:48000,bitrate:64000};
        const shapes=[
            [base,true],[{...base,samplerate:8000},true],[{...base,samplerate:24000},true],
            [{...base,samplerate:44100},false],[{...base,samplerate:96000},false],
            [{...base,channels:'3'},false],[{...base,channels:'stereo'},false],
            [{...base,bitrate:499},false],[{...base,bitrate:512001},false]
        ];
        Promise.all(shapes.map(([audio])=>cap.encodingInfo({type:'record',audio}))).then(reports=>{
            for(let index=0;index<reports.length;index++)
                if(reports[index].supported!==shapes[index][1])throw Error('worker capture bounds');
            postMessage('passed');
        });
    "#,
    );
}

#[test]
fn worker_queries_are_tasks_after_microtasks_with_deep_owned_snapshots() {
    let (mut runtime, initial) = start(
        r#"
        const cap=navigator.mediaCapabilities,order=[];
        const audio={contentType:'audio/webm;codecs=opus',channels:2,samplerate:48000};
        const configuration={type:'record',audio};
        const promise=cap.encodingInfo(configuration);
        order.push('returned'); audio.channels=9;configuration.type='webrtc';
        Promise.resolve().then(()=>order.push('microtask'));
        promise.then(report=>{
            order.push('query');
            if(!report.supported || report.configuration===configuration || report.configuration.audio===audio ||
                report.configuration.audio.channels!=='2' || report.configuration.type!=='record')
                throw Error('worker snapshot');
            postMessage(order.join(','));
        });
    "#,
    );
    assert!(
        initial.messages.is_empty(),
        "query may not settle in its calling task"
    );
    assert_eq!(runtime.next_timer_delay(), Some(Duration::ZERO));
    let outcome = runtime.advance_time(Duration::ZERO, 1);
    check(&outcome);
    assert_eq!(outcome.messages, ["\"returned,microtask,query\""]);
    assert_eq!(runtime.next_timer_delay(), None);
}

#[test]
fn worker_author_timer_replacement_and_clear_timeout_cannot_cancel_capability_tasks() {
    verify(
        r#"
        self.setTimeout=()=>{throw Error('author timer replacement');};
        const promise=navigator.mediaCapabilities.encodingInfo({type:'record',
            audio:{contentType:'audio/webm;codecs=opus'}});
        // The scheduler owns a private callback. A guessed timer ID is not an
        // author-created timeout and must not cancel the capability task.
        clearTimeout(1);clearInterval(1);
        promise.then(report=>{if(!report.supported)throw Error('query');postMessage('passed');});
    "#,
    );
}

#[test]
fn worker_eme_queries_reject_invalid_state_after_conversion_not_device_access() {
    verify(
        r#"
        const cap=navigator.mediaCapabilities;
        const keySystemConfiguration={keySystem:'org.w3.clearkey'};
        cap.decodingInfo({type:'file',audio:{contentType:'audio/webm;codecs=opus'},keySystemConfiguration})
        .then(()=>{throw Error('EME exposed in worker');},error=>{
            if(error.name!=='InvalidStateError')throw Error('worker EME error '+error.name);
            return cap.encodingInfo({type:'record',audio:{contentType:'audio/webm;codecs=opus'},
                get keySystemConfiguration(){throw Error('encoding must ignore EME dictionary');}});
        }).then(report=>{if(!report.supported)throw Error('encoding');postMessage('passed');});
    "#,
    );
}

#[test]
fn worker_invalid_dictionaries_and_getter_exceptions_are_rejected_promises() {
    verify(
        r#"
        const cap=navigator.mediaCapabilities,token={author:true},reads=[];
        const malformed=[null,{},1,{type:'record'},
            {type:'file',audio:{contentType:'audio/webm;codecs=opus'}},
            {type:'record',audio:{contentType:'audio/webm'}},
            {type:'record',audio:{contentType:'audio/webm;codecs=opus',bitrate:1n}}];
        const promises=malformed.map(config=>cap.encodingInfo(config).then(()=>false,
            error=>error.name==='TypeError'));
        promises.push(cap.encodingInfo({audio:{get bitrate(){reads.push('bitrate');throw token;},
            get contentType(){throw Error('late getter');}},get type(){throw Error('late type');}})
            .then(()=>false,error=>error===token && reads.join(',')==='bitrate'));
        Promise.all(promises).then(results=>{
            if(!results.every(Boolean))throw Error('worker conversion');postMessage('passed');
        });
    "#,
    );
}

#[test]
fn cancelling_worker_retires_pending_media_capability_callbacks() {
    let (mut runtime, initial) = start(
        r#"
        for(let index=0;index<8;index++)navigator.mediaCapabilities.encodingInfo({type:'record',
            audio:{contentType:'audio/webm;codecs=opus'}}).then(()=>postMessage('late'));
    "#,
    );
    assert!(initial.messages.is_empty());
    assert_eq!(runtime.next_timer_delay(), Some(Duration::ZERO));
    runtime.cancel();
    assert_eq!(runtime.next_timer_delay(), None);
    let after = runtime.advance_time(Duration::from_secs(1), 32);
    check(&after);
    assert!(after.messages.is_empty());
}
