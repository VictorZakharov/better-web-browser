//! Lossless packet transport through the asynchronous decoder interface.
use super::check;

#[test]
fn registered_pcm_decoders_preserve_interleaved_bytes_and_metadata() {
    check(
        r#"
        for(const [codec,format,width] of [['pcm-u8','u8',1],['pcm-s16','s16',2],
            ['pcm-s32','s32',4],['pcm-f32','f32',4]]) {
            const source=new Uint8Array(17*3*width);
            for(let i=0;i<source.length;i++)source[i]=(i*37+11)%256;
            const expected=source.slice(),outputs=[];
            assert((await AudioDecoder.isConfigSupported({codec,sampleRate:44100,numberOfChannels:3})).supported,
                codec+' decoder is supported');
            assert(!(await AudioEncoder.isConfigSupported({codec,sampleRate:44100,numberOfChannels:3})).supported,
                'no PCM encoder claim');
            const decoder=new AudioDecoder({output:frame=>outputs.push(frame),error:error=>{throw error;}});
            decoder.configure({codec,sampleRate:44100,numberOfChannels:3});
            const chunk=new EncodedAudioChunk({type:'key',timestamp:-12345,duration:999999,data:source});
            decoder.decode(chunk);source.fill(0);await decoder.flush();
            assert(outputs.length===1,'one packet gives one output');
            const audio=outputs[0],actual=new Uint8Array(expected.length);
            assert(audio.format===format&&audio.numberOfFrames===17&&audio.numberOfChannels===3,
                'native sample layout');
            assert(audio.sampleRate===44100&&audio.timestamp===-12345&&audio.duration===385,
                'duration comes from frames rather than authored duration');
            audio.copyTo(actual,{planeIndex:0});
            assert(actual.every((byte,index)=>byte===expected[index]),'byte-exact immutable packet transport');
            audio.close();decoder.close();
        }
    "#,
    );
}

#[test]
fn pcm_signed_24_bit_expands_without_rounding_or_channel_reordering() {
    check(
        r#"
        const bytes=new Uint8Array([0,0,128, 255,255,127, 255,255,255, 0,0,0, 1,0,0, 0,0,64]);
        const outputs=[];
        const decoder=new AudioDecoder({output:frame=>outputs.push(frame),error:error=>{throw error;}});
        decoder.configure({codec:'pcm-s24',sampleRate:48000,numberOfChannels:2});
        decoder.decode(new EncodedAudioChunk({type:'key',timestamp:750,data:bytes}));await decoder.flush();
        const audio=outputs[0],samples=new Int32Array(6);
        audio.copyTo(samples,{planeIndex:0});
        assert(audio.format==='s32'&&audio.numberOfFrames===3&&audio.timestamp===750,'24-bit output metadata');
        const expected=[-2147483648,2147483392,-256,0,256,1073741824];
        assert(samples.every((sample,index)=>sample===expected[index]),'exact left alignment');
        const left=new Float32Array(3);audio.copyTo(left,{planeIndex:0,format:'f32-planar'});
        assert(left[0]===-1&&left[1]===-1/8388608&&left[2]===1/8388608,'normalized channel extraction');
        audio.close();decoder.close();
    "#,
    );
}

#[test]
fn pcm_packet_validation_closes_asynchronously_without_partial_outputs() {
    check(
        r#"
        for(const data of [new Uint8Array(),new Uint8Array(5)]) {
            const errors=[],outputs=[];
            const decoder=new AudioDecoder({output:frame=>outputs.push(frame),error:error=>errors.push(error)});
            decoder.configure({codec:'pcm-s16',sampleRate:48000,numberOfChannels:2});
            decoder.decode(new EncodedAudioChunk({type:'key',timestamp:0,data}));
            assert(decoder.state==='configured','validation is on the control queue');
            let failure;try{await decoder.flush();}catch(error){failure=error;}
            assert(failure.name==='EncodingError'&&errors[0]===failure&&outputs.length===0,
                'malformed sample frames are never partially exposed');
            assert(decoder.state==='closed'&&decoder.decodeQueueSize===0,'fatal error clears queue');
        }
    "#,
    );
}

#[test]
fn pcm_flush_requires_a_new_key_but_keeps_the_decoder_configured() {
    check(
        r#"
        const outputs=[];
        const decoder=new AudioDecoder({output:frame=>outputs.push(frame),error:error=>{throw error;}});
        decoder.configure({codec:'pcm-u8',sampleRate:8000,numberOfChannels:1});
        const chunk=type=>new EncodedAudioChunk({type,timestamp:0,data:new Uint8Array([0,128,255])});
        let error;try{decoder.decode(chunk('delta'));}catch(e){error=e;}
        assert(error.name==='DataError'&&decoder.decodeQueueSize===0,'initial key required');
        decoder.decode(chunk('key'));decoder.decode(chunk('delta'));await decoder.flush();
        assert(outputs.length===2&&decoder.state==='configured','drained decoder remains configured');
        error=undefined;try{decoder.decode(chunk('delta'));}catch(e){error=e;}
        assert(error.name==='DataError','flush restores key requirement');
        decoder.decode(chunk('key'));await decoder.flush();assert(outputs.length===3,'new key resumes output');
        for(const audio of outputs)audio.close();decoder.close();
    "#,
    );
}
