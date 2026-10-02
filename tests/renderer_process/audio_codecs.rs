//! Native streaming codecs execute inside the existing hidden AppContainer renderer.
use super::support::*;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::RendererPresentation;
use std::time::{Duration, Instant};

fn completed_audio_page(session: &RendererSession, html: &str) -> RendererPresentation {
    let initial = load_html_document(session, 701, html);
    assert!(
        initial.runtime.errors.is_empty(),
        "{:?}",
        initial.runtime.errors
    );
    if initial.title == "audio passed" {
        return initial;
    }
    assert_eq!(initial.title, "pending");
    let document = initial.document;
    let mut next = initial.next_timer_micros;
    acknowledge(session, &initial);
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        assert!(Instant::now() < deadline, "isolated audio did not complete");
        session
            .advance_time(
                document,
                Duration::from_micros(next.unwrap_or(0).saturating_add(2_000)),
                64,
            )
            .unwrap();
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(page) if page.document == document => {
                assert!(page.runtime.errors.is_empty(), "{:?}", page.runtime.errors);
                assert!(!page.title.starts_with("failed:"), "{}", page.title);
                if page.title == "audio passed" {
                    return *page;
                }
                next = page.next_timer_micros;
                acknowledge(session, &page);
            }
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
                next = update.next_timer_micros;
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected isolated audio event: {event:?}"),
        }
        // Give the contained native worker CPU time; advancing the document's
        // logical clock alone must never be mistaken for worker completion.
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn native_opus_roundtrip_runs_in_the_app_container_without_a_media_broker_escape() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let page = completed_audio_page(
        &session,
        r#"<!doctype html><title>pending</title><script>
        (async()=>{
            const packets=[],frames=[];let config;
            const samples=new Float32Array(1920);for(let i=0;i<samples.length;i++)samples[i]=Math.sin(i*.1)*.2;
            const encoder=new AudioEncoder({output:(chunk,metadata)=>{packets.push(chunk);config??=metadata.decoderConfig;},
                error:error=>{document.title='failed:'+error.name;}});
            encoder.configure({codec:'opus',sampleRate:48000,numberOfChannels:1,opus:{format:'ogg'}});
            const audio=new AudioData({format:'f32',sampleRate:48000,numberOfChannels:1,
                numberOfFrames:1920,timestamp:0,data:samples});encoder.encode(audio);audio.close();await encoder.flush();
            const decoder=new AudioDecoder({output:audio=>frames.push(audio),error:error=>{document.title='failed:'+error.name;}});
            decoder.configure(config);for(const chunk of packets)decoder.decode(chunk);await decoder.flush();
            let energy=0,total=0;
            for(const frame of frames){const output=new Float32Array(frame.numberOfFrames);
                frame.copyTo(output,{planeIndex:0,format:'f32'});total+=output.length;
                for(const sample of output)energy+=sample*sample;frame.close();}
            encoder.close();decoder.close();
            if(energy<=1||total<1920)throw Error('no actual decoded audio');document.title='audio passed';
        })().catch(error=>document.title='failed:'+error.message);
    </script>"#,
    );
    acknowledge(&session, &page);
    session.cancel_document(page.document).unwrap();
    session.shutdown().unwrap();
}

#[test]
fn signed_24_bit_pcm_and_audio_resource_transfer_survive_renderer_ipc() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let page = completed_audio_page(
        &session,
        r#"<!doctype html><title>pending</title><script>
        (async()=>{
            const outputs=[];const decoder=new AudioDecoder({output:audio=>outputs.push(audio),
                error:error=>document.title='failed:'+error.name});
            decoder.configure({codec:'pcm-s24',sampleRate:48000,numberOfChannels:2});
            decoder.decode(new EncodedAudioChunk({type:'key',timestamp:9,
                data:new Uint8Array([0,0,128,255,255,127,255,255,255,1,0,0])}));await decoder.flush();
            const source=outputs[0],received=structuredClone({a:source,b:source},{transfer:[source]});
            if(source.numberOfFrames!==0||received.a!==received.b)throw Error('transfer ownership');
            const actual=new Int32Array(4);received.a.copyTo(actual,{planeIndex:0});
            if(actual.join(',')!=='-2147483648,2147483392,-256,256')throw Error('24-bit PCM precision');
            received.a.close();decoder.close();document.title='audio passed';
        })().catch(error=>document.title='failed:'+error.message);
    </script>"#,
    );
    acknowledge(&session, &page);
    session.cancel_document(page.document).unwrap();
    session.shutdown().unwrap();
}

#[test]
fn reset_aborts_flush_and_allows_a_new_native_codec_inside_the_renderer() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let page = completed_audio_page(
        &session,
        r#"<!doctype html><title>pending</title><script>
        (async()=>{
            let outputs=0,errors=0;
            const decoder=new AudioDecoder({output:audio=>{outputs++;audio.close();},error:()=>errors++});
            const config={codec:'pcm-u8',sampleRate:8000,numberOfChannels:1};
            decoder.configure(config);decoder.decode(new EncodedAudioChunk({type:'key',timestamp:0,data:new Uint8Array([128])}));
            const pending=decoder.flush();decoder.reset();let failure;try{await pending;}catch(error){failure=error;}
            if(failure?.name!=='AbortError'||outputs!==0||errors!==0)throw Error('reset did not abort');
            decoder.configure(config);decoder.decode(new EncodedAudioChunk({type:'key',timestamp:0,data:new Uint8Array([128])}));
            await decoder.flush();if(outputs!==1||errors!==0)throw Error('new decoder failed');
            decoder.close();document.title='audio passed';
        })().catch(error=>document.title='failed:'+error.message);
    </script>"#,
    );
    acknowledge(&session, &page);
    session.cancel_document(page.document).unwrap();
    session.shutdown().unwrap();
}

#[test]
fn message_ports_transfer_native_decoded_audio_without_losing_private_samples() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let page = completed_audio_page(
        &session,
        r#"<!doctype html><title>pending</title><script>
        (async()=>{
            const channel=new MessageChannel(), received=new Promise(resolve=>channel.port2.onmessage=resolve);
            const decoder=new AudioDecoder({output:audio=>channel.port1.postMessage({audio,alias:audio},[audio]),
                error:error=>{throw error;}});
            decoder.configure({codec:'pcm-s16',sampleRate:8000,numberOfChannels:2});
            decoder.decode(new EncodedAudioChunk({type:'key',timestamp:-9,
                data:new Int16Array([-32768,32767,-1,1])}));await decoder.flush();
            const value=(await received).data;
            if(!(value.audio instanceof AudioData)||value.audio!==value.alias)throw Error('native clone identity');
            const samples=new Int16Array(4);value.audio.copyTo(samples,{planeIndex:0});
            if(samples.join(',')!=='-32768,32767,-1,1'||value.audio.timestamp!==-9)throw Error('native clone samples');
            value.audio.close();decoder.close();channel.port1.close();channel.port2.close();document.title='audio passed';
        })().catch(error=>document.title='failed:'+error.message);
    </script>"#,
    );
    acknowledge(&session, &page);
    session.cancel_document(page.document).unwrap();
    session.shutdown().unwrap();
}
