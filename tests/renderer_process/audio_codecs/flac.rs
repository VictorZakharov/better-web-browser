//! The raw-frame encoder/decoder must work inside the actual sandbox, not only V8 tests.
use super::*;

#[test]
fn elementary_flac_roundtrip_and_partial_flush_are_contained_in_the_renderer() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let page = completed_audio_page(
        &session,
        r#"<!doctype html><title>pending</title><script>
    (async()=>{
        const packets=[];let metadata, failure;
        const encoder=new AudioEncoder({output:(chunk,info)=>{
            packets.push(chunk);metadata??=info.decoderConfig;
        },error:error=>{failure=error;}});
        encoder.configure({codec:'flac',sampleRate:48000,numberOfChannels:2,
            flac:{blockSize:512,compressLevel:8}});
        const source=new Int16Array(2050);
        for(let index=0;index<source.length;index++)source[index]=Math.round(Math.sin(index*.1)*8000);
        const input=new AudioData({format:'s16',sampleRate:48000,numberOfChannels:2,
            numberOfFrames:1025,timestamp:-1000,data:source});
        encoder.encode(input);input.close();await encoder.flush();if(failure)throw failure;
        if(packets.length!==3||packets[2].duration!==20)throw Error('incorrect flush tail');
        if(metadata.codec!=='flac'||metadata.description.byteLength!==42)throw Error('missing metadata');
        const recovered=[];
        const decoder=new AudioDecoder({output:audio=>{
            if(audio.format!=='s32-planar'||audio.sampleRate!==48000||audio.numberOfChannels!==2)
                throw Error('wrong actual output format');
            const values=new Int32Array(audio.numberOfFrames*2);
            audio.copyTo(values,{planeIndex:0,format:'s32'});
            recovered.push(...values);audio.close();
        },error:error=>{failure=error;}});
        decoder.configure({...metadata,sampleRate:1,numberOfChannels:32});
        for(const packet of packets)decoder.decode(packet);await decoder.flush();if(failure)throw failure;
        if(recovered.length!==source.length)throw Error('lost or padded samples');
        for(let index=0;index<source.length;index++)if(recovered[index]!==source[index]*65536)
            throw Error('integer precision lost');
        encoder.close();decoder.close();document.title='audio passed';
    })().catch(error=>document.title='failed:'+error.message);
    </script>"#,
    );
    acknowledge(&session, &page);
    session.cancel_document(page.document).unwrap();
    session.shutdown().unwrap();
}

#[test]
fn codec_replacement_drain_keeps_old_flac_metadata_before_opus_output() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let page = completed_audio_page(
        &session,
        r#"<!doctype html><title>pending</title><script>
    (async()=>{
        const codecs=[],durations=[];let failure;
        const encoder=new AudioEncoder({output:(chunk,metadata)=>{
            codecs.push(metadata.decoderConfig?.codec);durations.push(chunk.duration);
        },error:error=>{failure=error;}});
        encoder.configure({codec:'flac',sampleRate:48000,numberOfChannels:1,flac:{blockSize:512}});
        const first=new AudioData({format:'f32',sampleRate:48000,numberOfChannels:1,
            numberOfFrames:17,timestamp:100,data:new Float32Array(17)});
        encoder.encode(first);first.close();
        encoder.configure({codec:'opus',sampleRate:48000,numberOfChannels:1});
        const second=new AudioData({format:'f32',sampleRate:48000,numberOfChannels:1,
            numberOfFrames:960,timestamp:900,data:new Float32Array(960)});
        encoder.encode(second);second.close();await encoder.flush();if(failure)throw failure;
        if(codecs.filter(Boolean).join(',')!=='flac,opus'||durations[0]!==354)
            throw Error('old codec drain used new config: '+codecs.join(',')+' / '+durations.join(','));
        encoder.close();document.title='audio passed';
    })().catch(error=>document.title='failed:'+error.message);
    </script>"#,
    );
    acknowledge(&session, &page);
    session.cancel_document(page.document).unwrap();
    session.shutdown().unwrap();
}

#[test]
fn crc_failure_is_reported_without_terminating_the_app_container_renderer() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let page = completed_audio_page(
        &session,
        r#"<!doctype html><title>pending</title><script>
    (async()=>{
        let packet,metadata;
        const encoder=new AudioEncoder({output:(chunk,info)=>{packet=chunk;metadata=info.decoderConfig;},
            error:error=>{throw error;}});
        encoder.configure({codec:'flac',sampleRate:48000,numberOfChannels:1,flac:{blockSize:32}});
        const source=new AudioData({format:'f32',sampleRate:48000,numberOfChannels:1,
            numberOfFrames:32,timestamp:0,data:new Float32Array(32)});
        encoder.encode(source);source.close();await encoder.flush();encoder.close();
        const bytes=new Uint8Array(packet.byteLength);packet.copyTo(bytes);bytes[bytes.length-1]^=1;
        let errors=0,outputs=0,failure;
        const decoder=new AudioDecoder({output:audio=>{outputs++;audio.close();},
            error:error=>{errors++;failure=error;}});
        decoder.configure(metadata);decoder.decode(new EncodedAudioChunk({type:'key',timestamp:0,data:bytes}));
        let rejected;try{await decoder.flush();}catch(error){rejected=error;}
        if(outputs||errors!==1||decoder.state!=='closed'||failure?.name!=='EncodingError'
            ||rejected?.name!=='EncodingError')throw Error('missing contained codec failure');
        const followup=new AudioDecoder({output:audio=>{outputs++;audio.close();},error:error=>{throw error;}});
        followup.configure({codec:'pcm-u8',sampleRate:8000,numberOfChannels:1});
        followup.decode(new EncodedAudioChunk({type:'key',timestamp:0,data:new Uint8Array([128])}));
        await followup.flush();followup.close();if(outputs!==1)throw Error('renderer stopped');
        document.title='audio passed';
    })().catch(error=>document.title='failed:'+error.message);
    </script>"#,
    );
    acknowledge(&session, &page);
    session.cancel_document(page.document).unwrap();
    session.shutdown().unwrap();
}
