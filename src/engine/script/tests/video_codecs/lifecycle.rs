//! Key-frame admission, generation cancellation and asynchronous failure delivery.
use super::*;

#[test]
fn reset_cancels_native_work_and_rejects_old_flush_without_emitting_stale_frames() {
    check(&format!(
        r#"
        {}
        let count=0,errors=0;
        const decoder=new VideoDecoder({{output:frame=>{{count++;frame.close();}},error:()=>errors++}});
        const config={{codec:'av01.0.04M.08'}};decoder.configure(config);
        decoder.decode(new EncodedVideoChunk({{type:'key',timestamp:0,data:new Uint8Array(videoPackets[0])}}));
        const pending=decoder.flush();decoder.reset();
        let failure;try{{await pending;}}catch(error){{failure=error;}}
        assert(failure?.name==='AbortError'&&count===0&&errors===0&&decoder.state==='unconfigured','generation cancelled');
        decoder.configure(config);
        decoder.decode(new EncodedVideoChunk({{type:'key',timestamp:-9,data:new Uint8Array(videoPackets[0])}}));
        await decoder.flush();assert(count===1&&errors===0,'fresh native generation');decoder.close();
    "#,
        source()
    ));
}

#[test]
fn delta_chunks_need_a_preceding_key_after_configuration_reset_and_flush() {
    check(&format!(
        r#"
        {}
        const decoder=new VideoDecoder({{output:frame=>frame.close(),error:error=>{{throw error;}}}});
        const config={{codec:'av01.0.04M.08'}};decoder.configure(config);
        const delta=new EncodedVideoChunk({{type:'delta',timestamp:0,data:new Uint8Array(videoPackets[1])}});
        const rejectDelta=()=>{{let failure;try{{decoder.decode(delta);}}catch(error){{failure=error;}}
            assert(failure?.name==='DataError','fresh key required');}};
        rejectDelta();
        decoder.decode(new EncodedVideoChunk({{type:'key',timestamp:0,data:new Uint8Array(videoPackets[0])}}));
        await decoder.flush();rejectDelta();
        decoder.reset();decoder.configure(config);rejectDelta();decoder.close();
    "#,
        source()
    ));
}

#[test]
fn native_decode_error_closes_only_the_codec_and_rejects_flush_with_the_same_exception() {
    check(
        r#"
        let outputs=0,failure;
        const decoder=new VideoDecoder({output:frame=>{outputs++;frame.close();},error:error=>{failure=error;}});
        decoder.configure({codec:'av01.0.04M.08'});
        decoder.decode(new EncodedVideoChunk({type:'key',timestamp:0,data:new Uint8Array([255,255,255])}));
        let rejected;try{await decoder.flush();}catch(error){rejected=error;}
        assert(outputs===0&&failure===rejected&&failure?.name==='EncodingError'&&decoder.state==='closed',
            'one real native failure, no fake output');
        assert((await VideoDecoder.isConfigSupported({codec:'av01.0.04M.08'})).supported,'realm remains usable');
    "#,
    );
}
