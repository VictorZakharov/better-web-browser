//! Dedicated workers own decoder jobs and frame resources independently of DOM.
use super::*;
use std::time::Instant;

fn start_at(
    url: &str,
    source: &str,
    secure_creator: bool,
) -> (WorkerRuntime, WorkerRuntimeOutcome) {
    let (runtime, outcome) = WorkerRuntime::start_with_creator_context(
        url,
        source,
        "frames",
        ScriptKind::Classic,
        Arc::new(|url, _| Err(format!("unexpected worker import {url}"))),
        Arc::new(crate::fetch::csp::PolicyContainer::default()),
        secure_creator,
    );
    verify_outcome(&outcome);
    (runtime.expect("frame worker initializes"), outcome)
}

fn verify_outcome(outcome: &WorkerRuntimeOutcome) {
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(
        !outcome
            .console
            .iter()
            .any(|line| line.starts_with("error:")),
        "{:?}",
        outcome.console
    );
    assert!(
        outcome.fetch_actions.is_empty(),
        "image decode must not fetch"
    );
    assert!(
        outcome.database_actions.is_empty(),
        "frames must not touch origin storage"
    );
    assert!(outcome.websocket_actions.is_empty());
}

fn run(
    source: &str,
) -> (
    WorkerRuntime,
    Vec<super::super::worker_message::WorkerMessage>,
) {
    let fixtures = serde_json::from_str::<serde_json::Value>(include_str!(
        "../../../../tests/image-frame-fixtures/frames.json"
    ))
    .unwrap();
    let source = format!(
        r#"
        const assert=(value,message)=>{{if(!value)throw Error(message);}};
        const fixtureBytes=name=>Uint8Array.from(atob(({fixtures}).find(item=>item.name===name).bytes),c=>c.charCodeAt(0));
        (async()=>{{{source}}})().then(()=>postMessage('passed'),e=>console.error(e.name+':'+e.message));
    "#
    );
    let (mut runtime, initial) = start_at("https://example.test/worker.js", &source, true);
    let mut messages = initial.messages;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !messages.iter().any(|message| message == "\"passed\"") {
        assert!(
            Instant::now() < deadline,
            "worker image test timed out: {messages:?}"
        );
        std::thread::sleep(Duration::from_millis(2));
        let next = runtime.advance_time(Duration::from_millis(10), 64);
        verify_outcome(&next);
        messages.extend(next.messages);
    }
    (runtime, messages)
}

#[test]
fn worker_decodes_png_and_transfers_actual_frame_pixels() {
    let (_, messages) = run(r#"
        assert(typeof ImageDecoder==='function'&&typeof VideoFrame==='function','worker APIs');
        const decoder=new ImageDecoder({type:'image/png',data:fixtureBytes('rgba.png')});
        const {image}=await decoder.decode();
        assert(image.codedWidth===2&&image.codedHeight===1,'worker decode dimensions');
        const output=new Uint8Array(image.allocationSize());await image.copyTo(output);
        assert(output.join(',')==='10,20,30,255,40,50,60,128','worker decoder pixels');
        postMessage({frame:image},[image]);
        assert(image.codedWidth===0,'sender frame closes on worker transfer');decoder.close();
    "#);
    let value: serde_json::Value = serde_json::from_str(&messages[0]).unwrap();
    assert_eq!(value["t"], "object");
    let frame = &value["v"][0][1];
    assert_eq!(frame["t"], "video-frame");
    assert!(frame.to_string().contains("RGBA"));
}

#[test]
fn worker_stream_decoder_uses_worker_readable_stream_bindings() {
    run(r#"
        const bytes=fixtureBytes('rgba.png');
        const stream=new ReadableStream({start(c){c.enqueue(bytes.subarray(0,7));c.enqueue(bytes.subarray(7));c.close();}});
        const decoder=new ImageDecoder({type:'image/png',data:stream});
        const {image}=await decoder.decode();
        assert(decoder.complete&&!stream.locked&&image.codedWidth===2,'worker streamed decode');
        image.close();decoder.close();
        for(const key of ['__imageDecoderStreamBindings','__bindImageDecoderStreams','__videoFrameCloneBindings'])
            assert(!(key in globalThis),'private handoff removed '+key);
    "#);
}

#[test]
fn worker_draws_a_rotated_frame_into_offscreen_canvas() {
    run(r#"
        const frame=new VideoFrame(new Uint8Array([1,0,0,255,2,0,0,255]),
            {format:'RGBA',codedWidth:2,codedHeight:1,timestamp:3,rotation:90});
        const canvas=new OffscreenCanvas(1,2),context=canvas.getContext('2d');context.drawImage(frame,0,0);
        assert([...context.getImageData(0,0,1,2).data].join(',')==='1,0,0,255,2,0,0,255','worker Canvas pixels');
        const bitmap=await createImageBitmap(frame);frame.close();
        assert(bitmap.width===1&&bitmap.height===2,'worker bitmap snapshot');bitmap.close();
    "#);
}

#[test]
fn worker_clone_and_transfer_preserve_reference_graphs() {
    run(r#"
        const source=new VideoFrame(new Uint8Array([9,8,7,255]),{format:'RGBA',codedWidth:1,codedHeight:1,timestamp:5});
        const cloned=structuredClone({a:source,b:source});
        assert(cloned.a===cloned.b&&source.codedWidth===1,'worker clone alias and source lifetime');
        const moved=structuredClone(cloned.a,{transfer:[cloned.a]});
        assert(cloned.a.codedWidth===0&&moved.timestamp===5,'worker transfer');
        const pixels=new Uint8Array(4);await moved.copyTo(pixels);
        assert(pixels.join(',')==='9,8,7,255','worker transferred resource');source.close();moved.close();
    "#);
}

#[test]
fn worker_animation_random_access_uses_isolated_native_session() {
    run(r#"
        for(const [name,type] of [['composition.png','image/png'],['animation.webp','image/webp']]) {
            const decoder=new ImageDecoder({type,data:fixtureBytes(name)});
            const {image}=await decoder.decode({frameIndex:2});
            assert(image.timestamp===80000&&decoder.tracks.selectedTrack.frameCount===3,'worker animation metadata');
            image.close();decoder.close();
        }
    "#);
}

#[test]
fn insecure_worker_and_insecure_creator_do_not_expose_image_decoder() {
    for (url, creator) in [
        ("http://example.test/worker.js", true),
        ("https://example.test/worker.js", false),
    ] {
        let (_, outcome) = start_at(
            url,
            r#"postMessage(typeof ImageDecoder+'|'+typeof VideoFrame);"#,
            creator,
        );
        assert_eq!(outcome.messages, ["\"undefined|function\""]);
    }
    let (_, outcome) = start_at(
        "http://127.0.0.1/worker.js",
        "postMessage(typeof ImageDecoder);",
        true,
    );
    assert_eq!(outcome.messages, ["\"function\""]);
}

#[test]
fn cancel_worker_clears_decoder_jobs_and_scheduled_polling() {
    let (mut runtime, initial) = start_at(
        "https://example.test/worker.js",
        r#"
        const decoder=new ImageDecoder({type:'image/png',data:new ReadableStream()});
        decoder.decode().catch(()=>{});setInterval(()=>{},10);
    "#,
        true,
    );
    assert!(initial.messages.is_empty());
    assert!(runtime.next_timer_delay().is_some());
    runtime.cancel();
    assert!(runtime.next_timer_delay().is_none());
    let outcome = runtime.advance_time(Duration::from_secs(1), 64);
    assert!(outcome.closed && outcome.messages.is_empty());
}

#[test]
fn incoming_worker_frame_can_be_drawn_without_exposing_serialization_capabilities() {
    let (mut runtime, initial) = start_at(
        "https://example.test/worker.js",
        r#"
        onmessage=async event=>{
            const frame=event.data;
            const pixels=new Uint8Array(frame.allocationSize());await frame.copyTo(pixels);
            postMessage(pixels.join(','));frame.close();
        };
    "#,
        true,
    );
    assert!(initial.messages.is_empty());
    let serialized = serde_json::json!({"t":"video-frame","id":1,"p":"CQgH/w==","v":{
        "format":"RGBA","width":1,"height":1,"visible":{"x":0,"y":0,"width":1,"height":1},
        "displayWidth":1,"displayHeight":1,"timestamp":1,"duration":null,"rotation":0,"flip":false,
        "color":{"primaries":"bt709","transfer":"iec61966-2-1","matrix":"rgb","fullRange":true},
        "planeCount":1
    }})
    .to_string();
    let outcome = runtime.dispatch_message(&serialized);
    verify_outcome(&outcome);
    assert_eq!(outcome.messages, ["\"9,8,7,255\""]);
}
