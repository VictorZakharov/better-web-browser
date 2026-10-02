//! Real V8/bootstrap/native worker tests, driven by the existing task scheduler.
use super::*;
use std::time::{Duration, Instant};

mod compressed;
mod compressed_fixture;
mod configuration;
mod lifecycle;
mod messaging;
mod pcm;
mod pipelines;
mod reconfiguration;
mod worker_compressed;
mod workers;

pub(super) fn check(source: &str) -> Vec<String> {
    check_with_errors(source, &[])
}

pub(super) fn worker_check(source: &str) {
    workers::worker_check(source);
}

fn check_with_errors(source: &str, expected_errors: &[&str]) -> Vec<String> {
    let html = format!(
        r#"<body><script>
        const assert=(value,message)=>{{if(!value)throw Error(message);}};
        const tone=(frames=960,channels=1,rate=48000,timestamp=0)=>{{
            const samples=new Float32Array(frames*channels);
            for(let frame=0;frame<frames;frame++)for(let channel=0;channel<channels;channel++)
                samples[frame*channels+channel]=Math.sin(frame*(330+channel*110)*Math.PI*2/rate)*.25;
            return new AudioData({{format:'f32',sampleRate:rate,numberOfChannels:channels,
                numberOfFrames:frames,timestamp,data:samples}});
        }};
        (async()=>{{{source}}})().then(()=>console.log('audio codecs passed'),
            error=>console.error(error.name+':'+error.message));
        </script></body>"#
    );
    let dom = dom::parse_with_scripting(&html, true);
    let scripts = dom
        .elements_named("script")
        .map(|node| ScriptInput {
            source_url: "https://example.com/#inline".into(),
            code: node.text_content(),
            node,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        })
        .collect::<Vec<_>>();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial(&scripts);
    let mut errors = outcome.errors;
    let mut console = outcome.console;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !console
        .iter()
        .any(|line| line == "log: audio codecs passed")
    {
        assert!(
            !console.iter().any(|line| line.starts_with("error:")),
            "{console:?}"
        );
        assert!(
            Instant::now() < deadline,
            "audio codec tasks did not settle: {console:?}; {errors:?}"
        );
        std::thread::sleep(Duration::from_millis(2));
        let outcome = runtime.advance_time(Duration::from_millis(10), 64);
        errors.extend(outcome.errors);
        console.extend(outcome.console);
    }
    assert!(
        !console.iter().any(|line| line.starts_with("error:")),
        "{console:?}"
    );
    assert_eq!(errors.len(), expected_errors.len(), "{errors:?}");
    for (actual, expected) in errors.iter().zip(expected_errors) {
        assert!(actual.contains(expected), "{actual}");
    }
    console
}

#[test]
fn browser_opus_roundtrip_has_real_energy_and_metadata() {
    check(
        r#"
        const packets=[], pcm=[];let metadata;
        const encoder=new AudioEncoder({output:(packet,info)=>{packets.push(packet);metadata??=info.decoderConfig;},
            error:error=>{throw error;}});
        encoder.configure({codec:'opus',sampleRate:48000,numberOfChannels:2,bitrate:96000});
        const input=tone(4800,2);encoder.encode(input);input.close();
        assert(encoder.encodeQueueSize===1,'synchronous queue acceptance');
        await encoder.flush();
        assert(packets.length>=5&&encoder.encodeQueueSize===0,'drained actual packets');
        assert(metadata.codec==='opus'&&metadata.numberOfChannels===2,'decoder metadata');
        const decoder=new AudioDecoder({output:frame=>pcm.push(frame),error:error=>{throw error;}});
        decoder.configure(metadata);for(const packet of packets)decoder.decode(packet);
        await decoder.flush();
        let energy=0,frames=0;
        for(const audio of pcm){const samples=new Float32Array(audio.numberOfFrames*2);
            audio.copyTo(samples,{planeIndex:0,format:'f32'});frames+=audio.numberOfFrames;
            for(const sample of samples){assert(Number.isFinite(sample),'finite PCM');energy+=sample*sample;}
            audio.close();}
        assert(energy>1&&frames>=4800,'actual decoded sound');
        encoder.close();decoder.close();
    "#,
    );
}

#[test]
fn support_queries_clone_recognized_config_and_do_not_claim_other_codecs() {
    check(
        r#"
        const input={codec:'opus',sampleRate:48000,numberOfChannels:1,unknown:'ignored',
            opus:{format:'ogg',complexity:3,unknown:42}};
        const support=await AudioEncoder.isConfigSupported(input);
        assert(support.supported&&support.config.codec==='opus','real Opus capability');
        assert(!('unknown' in support.config)&&!('unknown' in support.config.opus),'recognized dictionaries');
        input.opus.complexity=10;assert(support.config.opus.complexity===3,'configuration snapshot');
        for(const codec of ['aac','flac','vorbis','not-a-codec']){
            const result=await AudioDecoder.isConfigSupported({codec,sampleRate:48000,numberOfChannels:1});
            assert(result.supported===false,'unsupported codec '+codec);
        }
        assert(!(await AudioEncoder.isConfigSupported({codec:'opus',sampleRate:44100,numberOfChannels:1})).supported,
            'no fake 44.1 kHz resampling claim');
    "#,
    );
}

#[test]
fn reset_rejects_flush_and_cancels_queued_outputs_immediately() {
    check(
        r#"
        let outputs=0,errors=0;
        const encoder=new AudioEncoder({output:()=>outputs++,error:()=>errors++});
        encoder.configure({codec:'opus',sampleRate:48000,numberOfChannels:1});
        encoder.encode(tone());const pending=encoder.flush();encoder.reset();
        assert(encoder.state==='unconfigured'&&encoder.encodeQueueSize===0,'synchronous reset');
        let error;try{await pending;}catch(e){error=e.name;}
        assert(error==='AbortError'&&outputs===0&&errors===0,'abort cancels author outputs');
        encoder.configure({codec:'opus',sampleRate:48000,numberOfChannels:1});
        encoder.encode(tone());await encoder.flush();assert(outputs>0,'reconfigured after reset');encoder.close();
    "#,
    );
}

#[test]
fn unsupported_configuration_closes_with_not_supported_and_same_flush_error() {
    check(
        r#"
        const errors=[];const decoder=new AudioDecoder({output:()=>{},error:e=>errors.push(e)});
        decoder.configure({codec:'unimplemented',sampleRate:48000,numberOfChannels:1});
        assert(decoder.state==='configured','configuration acceptance before asynchronous support check');
        let error;try{await decoder.flush();}catch(e){error=e;}
        assert(decoder.state==='closed'&&errors.length===1&&errors[0].name==='NotSupportedError','unsupported closes');
        assert(error===errors[0],'same fatal error in flush and callback');
    "#,
    );
}

#[test]
fn malformed_packet_is_an_asynchronous_encoding_error() {
    check(
        r#"
        const errors=[];const decoder=new AudioDecoder({output:()=>{throw Error('no concealment');},error:e=>errors.push(e)});
        decoder.configure({codec:'opus',sampleRate:48000,numberOfChannels:1});
        decoder.decode(new EncodedAudioChunk({type:'key',timestamp:0,data:new Uint8Array()}));
        let error;try{await decoder.flush();}catch(e){error=e;}
        assert(error.name==='EncodingError'&&errors.length===1&&decoder.state==='closed','invalid packet closes');
    "#,
    );
}

#[test]
fn ogg_format_packets_use_opushead_without_ogg_container_pages() {
    check(
        r#"
        const packets=[],pcm=[];let metadata;
        const encoder=new AudioEncoder({output:(chunk,info)=>{packets.push(chunk);metadata??=info.decoderConfig;},error:e=>{throw e;}});
        encoder.configure({codec:'opus',sampleRate:48000,numberOfChannels:1,opus:{format:'ogg'}});
        encoder.encode(tone(960));await encoder.flush();
        assert(metadata.description instanceof ArrayBuffer,'identification description');
        assert(String.fromCharCode(...new Uint8Array(metadata.description).slice(0,8))==='OpusHead','real identification');
        const decoder=new AudioDecoder({output:frame=>pcm.push(frame),error:e=>{throw e;}});
        decoder.configure(metadata);for(const packet of packets)decoder.decode(packet);await decoder.flush();
        assert(pcm[0].timestamp===0&&pcm[0].numberOfFrames<960,'initial pre-skip applied');
        for(const frame of pcm)frame.close();encoder.close();decoder.close();
    "#,
    );
}
