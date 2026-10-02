//! End-to-end elementary codecs through Web IDL, V8, queue tasks and native jobs.
use super::*;
use crate::engine::script::audio_codecs::test_packets;

#[test]
fn browser_decoders_report_actual_stream_dimensions_and_real_pcm_for_every_alias() {
    check(&format!(
        r#"
        {}
        for(const entry of elementaryCases){{
            const config={{codec:entry.codec,sampleRate:1,numberOfChannels:32}};
            if(entry.description!==null)config.description=new Uint8Array(entry.description);
            const support=await AudioDecoder.isConfigSupported(config);
            assert(support.supported,'registered codec '+entry.codec);
            assert(support.config.sampleRate===1&&support.config.numberOfChannels===32,'recognized nominal config');
            let frames=0,energy=0,count=0;
            const decoder=new AudioDecoder({{output:audio=>{{
                assert(audio.sampleRate===44100&&audio.numberOfChannels===1,'actual stream dimensions');
                assert(audio.timestamp===(count+(entry.primingPackets??0))*26122,'chunk presentation timestamp');
                assert(audio.duration===Math.floor(audio.numberOfFrames*1000000/44100),'duration uses actual rate');
                const pcm=new Float32Array(audio.numberOfFrames);
                audio.copyTo(pcm,{{planeIndex:0,format:'f32-planar'}});
                for(const sample of pcm){{assert(Number.isFinite(sample),'finite PCM');energy+=sample*sample;}}
                frames+=audio.numberOfFrames;count++;audio.close();
            }},error:error=>{{throw error;}}}});
            decoder.configure(config);
            let index=0;
            for(const bytes of entry.packets){{
                decoder.decode(new EncodedAudioChunk({{type:'key',timestamp:index++*26122,data:new Uint8Array(bytes)}}));
                if(decoder.decodeQueueSize===32)await decoder.flush();
            }}
            assert(decoder.decodeQueueSize===entry.packets.length%32,'synchronous bounded queue acceptance');
            await decoder.flush();
            assert(decoder.decodeQueueSize===0&&count===entry.packets.length-(entry.primingPackets??0),'complete packet drain');
            assert(frames>=entry.minimumFrames&&energy>1,'actual synthetic tone, not placeholder audio');
            decoder.close();
        }}
    "#,
        test_packets::javascript_cases()
    ));
}

#[test]
fn browser_flac_encoder_roundtrip_keeps_metadata_separate_and_flush_tail_unpadded() {
    check(
        r#"
        for(const channels of [1,2]){
            const config={codec:'flac',sampleRate:48000,numberOfChannels:channels,
                flac:{blockSize:256,compressLevel:5}};
            const support=await AudioEncoder.isConfigSupported(config);
            assert(support.supported,'real FLAC encoder');
            assert(support.config.flac.blockSize===256&&support.config.flac.compressLevel===5,'recognized FLAC options');
            const packets=[];let metadata;
            const encoder=new AudioEncoder({output:(chunk,info)=>{
                packets.push(chunk);metadata??=info.decoderConfig;
                assert(chunk.type==='key','independently decodable FLAC frame');
                const bytes=new Uint8Array(chunk.byteLength);chunk.copyTo(bytes);
                assert(bytes[0]===255&&(bytes[1]&254)===248,'elementary frame sync');
            },error:error=>{throw error;}});
            encoder.configure(config);
            const input=tone(777,channels,48000,-1000);encoder.encode(input);input.close();
            await encoder.flush();
            assert(packets.length===4&&metadata.codec==='flac','four actual frame packets');
            const description=new Uint8Array(metadata.description);
            assert(description.length===42&&String.fromCharCode(...description.slice(0,4))==='fLaC','separate stream metadata');
            assert(packets[3].duration===Math.floor(9*1000000/48000),'unpadded final duration');
            let frames=0,energy=0;
            const decoder=new AudioDecoder({output:audio=>{
                assert(audio.format==='s32-planar','integer lossless decode');
                assert(audio.sampleRate===48000&&audio.numberOfChannels===channels,'decoded metadata');
                const pcm=new Float32Array(audio.numberOfFrames*channels);
                audio.copyTo(pcm,{planeIndex:0,format:'f32'});
                for(const sample of pcm)energy+=sample*sample;
                frames+=audio.numberOfFrames;audio.close();
            },error:error=>{throw error;}});
            decoder.configure({...metadata,sampleRate:1,numberOfChannels:32});
            for(const packet of packets)decoder.decode(packet);
            await decoder.flush();
            assert(frames===777&&energy>1,'real lossless PCM with no padding');
            encoder.close();decoder.close();
        }
    "#,
    );
}

#[test]
fn description_views_are_snapshotted_before_author_mutation_or_buffer_transfer() {
    check(&format!(
        r#"
        {}
        for(const entry of elementaryCases.filter(item=>item.description!==null)){{
            const backing=new Uint8Array(entry.description.length+8);
            backing.set(entry.description,4);
            const description=new DataView(backing.buffer,4,entry.description.length);
            const request=AudioDecoder.isConfigSupported({{codec:entry.codec,sampleRate:1,
                numberOfChannels:32,description,unknown:42}});
            backing.fill(255);
            structuredClone(backing.buffer,{{transfer:[backing.buffer]}});
            const support=await request;
            assert(support.supported,'snapshot remains valid');
            assert(!('unknown' in support.config),'unrecognized members omitted');
            assert([...new Uint8Array(support.config.description.buffer)].join(',')===entry.description.join(','),
                'only original view bytes retained');
            let outputs=0;
            const decoder=new AudioDecoder({{output:audio=>{{outputs++;audio.close();}},error:error=>{{throw error;}}}});
            decoder.configure(support.config);
            support.config.description.fill(255);
            for(const bytes of entry.packets.slice(0,2))decoder.decode(new EncodedAudioChunk({{type:'key',timestamp:0,data:new Uint8Array(bytes)}}));
            await decoder.flush();assert(outputs===2-(entry.primingPackets??0),'configure also snapshots metadata');decoder.close();
        }}
    "#,
        test_packets::javascript_cases()
    ));
}

#[test]
fn unsupported_compressed_profiles_and_encoders_remain_honest() {
    check(
        r#"
        for(const codec of ['mp3','mp4a.69','mp4a.6B','mp4a.40.2','mp4a.40.02','mp4a.67']){
            assert(!(await AudioEncoder.isConfigSupported({codec,sampleRate:48000,numberOfChannels:1})).supported,
                'no unimplemented encoder '+codec);
        }
        for(const codec of ['mp4a.40.5','mp4a.40.05','mp4a.40.29','mp4a.40.42','MP3','mp4a.6b']){
            assert(!(await AudioDecoder.isConfigSupported({codec,sampleRate:48000,numberOfChannels:1})).supported,
                'no unsupported profile '+codec);
        }
        for(const description of [[],[0],[10,8],[26,8],[18,56],[18,0],[18,12]]){
            assert(!(await AudioDecoder.isConfigSupported({codec:'mp4a.40.2',sampleRate:48000,
                numberOfChannels:1,description:new Uint8Array(description)})).supported,'invalid ASC remains unsupported');
        }
        for(const options of [{sampleRate:192000},{numberOfChannels:3},{flac:{blockSize:16}},
            {flac:{blockSize:32768}},{flac:{compressLevel:9}},{bitrate:64000},{bitrateMode:'constant'}]){
            assert(!(await AudioEncoder.isConfigSupported({codec:'flac',sampleRate:48000,
                numberOfChannels:1,...options})).supported,'unsupported FLAC backend shape');
        }
    "#,
    );
}
