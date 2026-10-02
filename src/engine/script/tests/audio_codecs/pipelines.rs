//! Sample layouts must reach the real codec; a relabeled output is not sufficient.
use super::check;

#[test]
fn every_audio_data_sample_layout_reaches_real_stereo_opus_encoding() {
    check(
        r#"
        const formats=['u8','s16','s32','f32','u8-planar','s16-planar','s32-planar','f32-planar'];
        const types={u8:Uint8Array,s16:Int16Array,s32:Int32Array,f32:Float32Array};
        for(const format of formats) {
            const kind=format.split('-')[0],planar=format.endsWith('-planar'),frames=1920,channels=2;
            const samples=new types[kind](frames*channels);
            for(let frame=0;frame<frames;frame++)for(let channel=0;channel<channels;channel++) {
                const value=Math.sin(frame*(channel?660:330)*Math.PI*2/48000)*.3;
                const integer=kind==='u8'?Math.round(128+value*128):
                    kind==='s16'?Math.round(value*32768):kind==='s32'?Math.round(value*2147483648):value;
                samples[planar?channel*frames+frame:frame*channels+channel]=integer;
            }
            const input=new AudioData({format,sampleRate:48000,numberOfChannels:channels,
                numberOfFrames:frames,timestamp:12345,data:samples});
            const packets=[],outputs=[];let metadata;
            const encoder=new AudioEncoder({output:(chunk,info)=>{packets.push(chunk);metadata??=info.decoderConfig;},
                error:error=>{throw error;}});
            encoder.configure({codec:'opus',sampleRate:48000,numberOfChannels:2,bitrate:96000});
            encoder.encode(input);input.close();samples.fill(0);await encoder.flush();
            assert(packets.length>=2,'actual packets from '+format);
            const decoder=new AudioDecoder({output:audio=>outputs.push(audio),error:error=>{throw error;}});
            decoder.configure(metadata);for(const packet of packets)decoder.decode(packet);await decoder.flush();
            const energies=[0,0];
            for(const audio of outputs){for(let channel=0;channel<2;channel++){
                const plane=new Float32Array(audio.numberOfFrames);
                audio.copyTo(plane,{planeIndex:channel,format:'f32-planar'});
                for(const sample of plane){assert(Number.isFinite(sample),'finite '+format);energies[channel]+=sample*sample;}
            }audio.close();}
            assert(energies[0]>5&&energies[1]>5,'both channels have real audio '+format);
            encoder.close();decoder.close();
            // Wait for actual worker exit before opening the next pair. The
            // retained native permit intentionally outlives close().
            await new Promise(resolve=>setTimeout(resolve,5));
        }
    "#,
    );
}

#[test]
fn pcm_decoder_output_can_transfer_then_encode_without_author_byte_reconstruction() {
    check(
        r#"
        const channel=new MessageChannel(), received=new Promise(resolve=>channel.port2.onmessage=resolve);
        const bytes=new Uint8Array(960*2),view=new DataView(bytes.buffer);
        for(let i=0;i<960;i++)view.setInt16(i*2,Math.round(Math.sin(i*.07)*8192),true);
        const decoder=new AudioDecoder({output:audio=>channel.port1.postMessage(audio,[audio]),error:error=>{throw error;}});
        decoder.configure({codec:'pcm-s16',sampleRate:48000,numberOfChannels:1});
        decoder.decode(new EncodedAudioChunk({type:'key',timestamp:500,data:bytes}));await decoder.flush();
        const audio=(await received).data;assert(audio.format==='s16'&&audio.timestamp===500,'native resource arrives');
        const packets=[];const encoder=new AudioEncoder({output:chunk=>packets.push(chunk),error:error=>{throw error;}});
        encoder.configure({codec:'opus',sampleRate:48000,numberOfChannels:1});encoder.encode(audio);audio.close();
        await encoder.flush();assert(packets.length>=1&&packets[0].byteLength>0,'chained real codec');
        encoder.close();decoder.close();channel.port1.close();channel.port2.close();
    "#,
    );
}

#[test]
fn a_transferred_encoded_packet_decodes_identically_to_the_original_packet() {
    check(
        r#"
        const packets=[];let config;
        const encoder=new AudioEncoder({output:(packet,metadata)=>{packets.push(packet);config??=metadata.decoderConfig;},
            error:error=>{throw error;}});
        encoder.configure({codec:'opus',sampleRate:48000,numberOfChannels:1});
        const input=tone(960);encoder.encode(input);input.close();await encoder.flush();encoder.close();
        const channel=new MessageChannel(), delivered=new Promise(resolve=>channel.port2.onmessage=resolve);
        channel.port1.postMessage(packets);const cloned=(await delivered).data;
        const decode=async chunks=>{
            const output=[];const decoder=new AudioDecoder({output:audio=>{
                const values=new Float32Array(audio.numberOfFrames);audio.copyTo(values,{planeIndex:0});
                output.push(...values);audio.close();},error:error=>{throw error;}});
            decoder.configure(config);for(const chunk of chunks)decoder.decode(chunk);await decoder.flush();decoder.close();
            return output;
        };
        const before=await decode(packets),after=await decode(cloned);
        assert(before.length>0&&before.join(',')===after.join(','),'bit-identical decode after message cloning');
        channel.port1.close();channel.port2.close();
    "#,
    );
}

#[test]
fn consecutive_decoded_pcm_packets_keep_each_packet_timestamp_and_channel_order() {
    check(
        r#"
        const outputs=[];const decoder=new AudioDecoder({output:audio=>outputs.push(audio),error:error=>{throw error;}});
        decoder.configure({codec:'pcm-s32',sampleRate:32000,numberOfChannels:3});
        const timestamps=[-400,0,70000];
        for(let packet=0;packet<3;packet++){
            const samples=new Int32Array([packet+1,-packet-2,123456789,42,-42,2147483647]);
            decoder.decode(new EncodedAudioChunk({type:packet?'delta':'key',timestamp:timestamps[packet],data:samples}));
            samples.fill(0);
        }
        await decoder.flush();assert(outputs.length===3,'three distinct resources');
        for(let packet=0;packet<3;packet++){
            const audio=outputs[packet],samples=new Int32Array(6);audio.copyTo(samples,{planeIndex:0});
            assert(audio.timestamp===timestamps[packet]&&audio.duration===62,'packet timestamp and floor duration');
            assert(samples.join(',')===[packet+1,-packet-2,123456789,42,-42,2147483647].join(','),'channel bits');audio.close();
        }
        decoder.close();
    "#,
    );
}
