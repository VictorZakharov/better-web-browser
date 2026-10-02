//! Dedicated Worker realms use the same contained backend and native clone hooks.
use super::super::audio_codecs::worker_check;
use super::*;

#[test]
fn worker_retains_av1_reference_frames_and_transfers_decoded_resources() {
    worker_check(&format!(
        r#"
        {}
        const source={{codec:'av01.0.04M.08',codedWidth:16,codedHeight:16}};
        assert((await VideoDecoder.isConfigSupported(source)).supported,'worker capability');
        let outputs=0;
        const decoder=new VideoDecoder({{output:frame=>{{
            const clone=structuredClone({{a:frame,b:frame}},{{transfer:[frame]}});
            assert(frame.format===null&&clone.a===clone.b&&clone.a instanceof VideoFrame,'worker frame ownership');
            assert(clone.a.timestamp===outputs*250000,'worker packet timestamp');
            const bytes=new Uint8Array(clone.a.allocationSize());clone.a.copyTo(bytes);
            assert(bytes[0]>150&&bytes[3]===255,'actual worker pixels');
            outputs++;clone.a.close();
        }},error:error=>{{throw error;}}}});
        decoder.configure(source);
        for(let index=0;index<videoPackets.length;index++) {{
            const chunk=new EncodedVideoChunk({{type:index===0?'key':'delta',timestamp:index*250000,
                data:new Uint8Array(videoPackets[index])}});
            const copy=structuredClone(chunk);
            assert(copy instanceof EncodedVideoChunk&&!(copy instanceof EncodedAudioChunk),'worker video chunk brand');
            decoder.decode(copy);
        }}
        await decoder.flush();assert(outputs===8,'persistent worker references');decoder.close();
    "#,
        source()
    ));
}

#[test]
fn worker_codec_error_does_not_close_the_worker_or_break_later_audio_codecs() {
    worker_check(
        r#"
        let failed;
        const decoder=new VideoDecoder({output:frame=>{frame.close();throw Error('unexpected frame');},
            error:error=>failed=error});
        decoder.configure({codec:'av01.0.04M.08'});
        decoder.decode(new EncodedVideoChunk({type:'key',timestamp:0,data:new Uint8Array([255])}));
        let rejected;try{await decoder.flush();}catch(error){rejected=error;}
        assert(failed===rejected&&failed?.name==='EncodingError'&&decoder.state==='closed','worker native error');
        let audioFrames=0;
        const audio=new AudioDecoder({output:frame=>{audioFrames+=frame.numberOfFrames;frame.close();},error:error=>{throw error;}});
        audio.configure({codec:'pcm-u8',sampleRate:8000,numberOfChannels:1});
        audio.decode(new EncodedAudioChunk({type:'key',timestamp:0,data:new Uint8Array([128,128])}));
        await audio.flush();assert(audioFrames===2,'independent audio survives failed AV1');audio.close();
    "#,
    );
}
