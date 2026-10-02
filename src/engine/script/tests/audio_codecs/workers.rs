//! Dedicated workers use the same immutable resources and contained codec queue.
use super::*;
use std::sync::Arc;

pub(super) fn worker_check(source: &str) {
    let source = format!(
        r#"
        const assert=(value,message)=>{{if(!value)throw Error(message);}};
        (async()=>{{{source}}})().then(()=>console.log('worker audio passed'),
            error=>console.error(error.name+':'+error.message));
    "#
    );
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.com/audio.js",
        &source,
        "audio",
        ScriptKind::Classic,
        Arc::new(|_, _| Err("unexpected import".into())),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let mut runtime = runtime.unwrap();
    let mut console = initial.console;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !console
        .iter()
        .any(|line| line == "log: worker audio passed")
    {
        assert!(
            !console.iter().any(|line| line.starts_with("error:")),
            "{console:?}"
        );
        assert!(
            Instant::now() < deadline,
            "worker codec did not settle: {console:?}"
        );
        std::thread::sleep(Duration::from_millis(2));
        let outcome = runtime.advance_time(Duration::from_millis(10), 64);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        console.extend(outcome.console);
    }
    assert!(
        !console.iter().any(|line| line.starts_with("error:")),
        "{console:?}"
    );
}

#[test]
fn worker_opus_encoder_and_decoder_return_real_samples() {
    worker_check(
        r#"
        const input=new Float32Array(1920);
        for(let i=0;i<input.length;i++)input[i]=Math.sin(i*.1)*.2;
        const audio=new AudioData({format:'f32',sampleRate:48000,numberOfFrames:1920,
            numberOfChannels:1,timestamp:0,data:input});
        const packets=[],outputs=[];let metadata;
        const encoder=new AudioEncoder({output:(chunk,info)=>{packets.push(chunk);metadata??=info.decoderConfig;},
            error:error=>{throw error;}});
        encoder.configure({codec:'opus',sampleRate:48000,numberOfChannels:1});
        encoder.encode(audio);audio.close();await encoder.flush();
        assert(packets.length>=2,'worker encoded packets');
        const decoder=new AudioDecoder({output:frame=>outputs.push(frame),error:error=>{throw error;}});
        decoder.configure(metadata);for(const packet of packets)decoder.decode(packet);await decoder.flush();
        let energy=0;
        for(const frame of outputs){const samples=new Float32Array(frame.numberOfFrames);
            frame.copyTo(samples,{planeIndex:0,format:'f32'});for(const sample of samples)energy+=sample*sample;frame.close();}
        assert(energy>1,'worker sound is not a placeholder');encoder.close();decoder.close();
    "#,
    );
}

#[test]
fn worker_pcm_decoders_preserve_typed_sample_format() {
    worker_check(
        r#"
        for(const [codec,format,data] of [['pcm-u8','u8',new Uint8Array([0,128,255])],
            ['pcm-s16','s16',new Int16Array([-32768,0,32767])],
            ['pcm-s32','s32',new Int32Array([-2147483648,0,2147483647])],
            ['pcm-f32','f32',new Float32Array([-.5,0,.5])]]) {
            const outputs=[];
            const decoder=new AudioDecoder({output:audio=>outputs.push(audio),error:error=>{throw error;}});
            decoder.configure({codec,sampleRate:22050,numberOfChannels:1});
            decoder.decode(new EncodedAudioChunk({type:'key',timestamp:123,data}));await decoder.flush();
            assert(outputs.length===1&&outputs[0].format===format,'worker format');
            const actual=new data.constructor(3);outputs[0].copyTo(actual,{planeIndex:0});
            assert(actual.every((sample,index)=>sample===data[index]),'worker exact PCM');
            outputs[0].close();decoder.close();
        }
    "#,
    );
}

#[test]
fn document_transfers_audio_and_chunks_into_an_independent_worker_realm() {
    let (_, outcome) = execute_html(
        r#"<script>
        const worker=new Worker('/audio.js');
        const audio=new AudioData({format:'s16',sampleRate:8000,numberOfFrames:3,
            numberOfChannels:1,timestamp:-33,data:new Int16Array([-32768,0,32767])});
        const chunk=new EncodedAudioChunk({type:'key',timestamp:45,duration:7,data:new Uint8Array([1,2,3])});
        worker.postMessage({audio,alias:audio,chunk},{transfer:[audio]});
        console.log(audio.numberOfFrames);
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: 0"]);
    let ScriptWorkerAction::PostMessage { serialized, .. } = &outcome.worker_actions[1] else {
        panic!("expected worker postMessage")
    };
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.com/audio.js",
        r#"
        onmessage=event=>{
            const {audio,alias,chunk}=event.data;
            if(audio!==alias||audio.timestamp!==-33)throw Error('resource alias');
            const samples=new Int16Array(3);audio.copyTo(samples,{planeIndex:0});
            const packet=new Uint8Array(3);chunk.copyTo(packet);
            postMessage({samples:[...samples],packet:[...packet],format:audio.format,timestamp:chunk.timestamp});
            audio.close();
        };
    "#,
        "audio",
        ScriptKind::Classic,
        Arc::new(|_, _| Err("unexpected import".into())),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let received = runtime.unwrap().dispatch_message(serialized);
    assert!(received.errors.is_empty(), "{:?}", received.errors);
    assert_eq!(received.messages.len(), 1);
    assert!(
        received.messages[0].contains("-32768"),
        "{:?}",
        received.messages
    );
    assert!(received.messages[0].contains("32767"));
    assert!(received.messages[0].contains("s16"));
    assert!(received.messages[0].contains("45"));
}

#[test]
fn insecure_workers_have_sample_resources_but_no_codec_constructors() {
    let (_, outcome) = WorkerRuntime::start_with_creator_context(
        "https://example.com/audio.js",
        "console.log(typeof AudioData,typeof EncodedAudioChunk,typeof AudioEncoder,typeof AudioDecoder);",
        "audio",
        ScriptKind::Classic,
        Arc::new(|_, _| Err("unexpected import".into())),
        Arc::new(crate::fetch::csp::PolicyContainer::default()),
        false,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: function function undefined undefined"]
    );
}

#[test]
fn worker_close_cancels_pending_codec_callbacks() {
    let (runtime, outcome) = WorkerRuntime::start(
        "https://example.com/audio.js",
        r#"
        const decoder=new AudioDecoder({output:()=>{throw Error('output after close');},
            error:()=>{throw Error('error after close');}});
        decoder.configure({codec:'pcm-u8',sampleRate:8000,numberOfChannels:1});
        decoder.decode(new EncodedAudioChunk({type:'key',timestamp:0,data:new Uint8Array(1024)}));
        close();
    "#,
        "audio",
        ScriptKind::Classic,
        Arc::new(|_, _| Err("unexpected import".into())),
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.closed);
    if let Some(mut runtime) = runtime {
        let later = runtime.advance_time(Duration::from_secs(1), 64);
        assert!(later.errors.is_empty(), "{:?}", later.errors);
        assert!(later.console.is_empty());
    }
}
