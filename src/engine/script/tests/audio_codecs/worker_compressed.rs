//! Identical elementary contracts in a distinct dedicated-worker V8 realm.
use super::workers::worker_check;
use crate::engine::script::audio_codecs::test_packets;

#[test]
fn worker_decoders_deliver_actual_dimensions_and_sound_for_each_registered_codec() {
    worker_check(&format!(
        r#"
        {}
        for(const entry of elementaryCases){{
            const config={{codec:entry.codec,sampleRate:1,numberOfChannels:32}};
            if(entry.description!==null)config.description=new Uint8Array(entry.description);
            const support=await AudioDecoder.isConfigSupported(config);
            assert(support.supported,'worker codec '+entry.codec);
            let frames=0,energy=0;
            const decoder=new AudioDecoder({{output:audio=>{{
                assert(audio.sampleRate===44100&&audio.numberOfChannels===1,'stream dimensions in worker');
                const received=structuredClone(audio,{{transfer:[audio]}});
                assert(audio.numberOfFrames===0,'worker native sample ownership');
                const samples=new Float32Array(received.numberOfFrames);
                received.copyTo(samples,{{planeIndex:0,format:'f32-planar'}});
                for(const sample of samples){{assert(Number.isFinite(sample),'finite sample');energy+=sample*sample;}}
                frames+=received.numberOfFrames;received.close();
            }},error:error=>{{throw error;}}}});
            decoder.configure(config);
            for(const bytes of entry.packets){{
                decoder.decode(new EncodedAudioChunk({{type:'key',timestamp:0,data:new Uint8Array(bytes)}}));
                if(decoder.decodeQueueSize===32)await decoder.flush();
            }}
            await decoder.flush();
            assert(frames>=entry.minimumFrames&&energy>1,'actual worker decoder samples');decoder.close();
        }}
    "#,
        test_packets::javascript_cases()
    ));
}

#[test]
fn worker_flac_encoder_produces_native_lossless_frames_with_no_final_padding() {
    worker_check(
        r#"
        const support=await AudioEncoder.isConfigSupported({codec:'flac',sampleRate:44100,
            numberOfChannels:1,flac:{blockSize:512,compressLevel:3}});
        assert(support.supported,'native worker FLAC encoder');
        const packets=[];let metadata;
        const encoder=new AudioEncoder({output:(packet,info)=>{packets.push(packet);metadata??=info.decoderConfig;},
            error:error=>{throw error;}});
        encoder.configure(support.config);
        const input=new Int16Array(1025);
        for(let index=0;index<input.length;index++)input[index]=Math.round(Math.sin(index*.1)*8192);
        const audio=new AudioData({format:'s16',sampleRate:44100,numberOfChannels:1,
            numberOfFrames:1025,timestamp:-500,data:input});encoder.encode(audio);audio.close();await encoder.flush();
        assert(packets.length===3&&packets[2].duration===Math.floor(1000000/44100),'one-sample tail');
        assert(metadata.codec==='flac'&&metadata.description.byteLength===42,'worker decoder metadata');
        const received=[];
        const decoder=new AudioDecoder({output:frame=>{
            const pcm=new Int32Array(frame.numberOfFrames);frame.copyTo(pcm,{planeIndex:0});
            received.push(...pcm);frame.close();
        },error:error=>{throw error;}});
        decoder.configure({...metadata,sampleRate:1,numberOfChannels:32});
        for(const packet of packets)decoder.decode(structuredClone(packet));
        await decoder.flush();
        assert(received.length===input.length,'worker lossless exact frame count');
        assert(received.every((sample,index)=>sample===input[index]*65536),'worker signed integer precision');
        encoder.close();decoder.close();
    "#,
    );
}

#[test]
fn worker_decoder_errors_close_only_the_failed_codec_and_not_the_worker_realm() {
    worker_check(
        r#"
        let failures=0,output=0;
        const decoder=new AudioDecoder({output:()=>output++,error:error=>{
            assert(error.name==='EncodingError','native decoding error');failures++;
        }});
        decoder.configure({codec:'mp3',sampleRate:1,numberOfChannels:32});
        decoder.decode(new EncodedAudioChunk({type:'key',timestamp:0,data:new Uint8Array([255])}));
        let failure;try{await decoder.flush();}catch(error){failure=error;}
        assert(failure?.name==='EncodingError'&&decoder.state==='closed','failed codec closes');
        assert(failures===1&&output===0,'exactly one failure, no invented audio');
        const followup=new AudioDecoder({output:audio=>{output++;audio.close();},error:error=>{throw error;}});
        followup.configure({codec:'pcm-u8',sampleRate:8000,numberOfChannels:1});
        followup.decode(new EncodedAudioChunk({type:'key',timestamp:0,data:new Uint8Array([128])}));
        await followup.flush();assert(output===1,'realm remains usable');followup.close();
    "#,
    );
}
