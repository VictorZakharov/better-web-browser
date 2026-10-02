use super::{check, check_with_errors};

#[test]
fn author_output_exception_is_reported_without_closing_the_codec_or_losing_flush() {
    check_with_errors(
        r#"
        let outputs=0,errors=0;
        const decoder=new AudioDecoder({output:audio=>{audio.close();outputs++;
            if(outputs===1)throw Error('owned output callback failure');},error:()=>errors++});
        decoder.configure({codec:'pcm-u8',sampleRate:8000,numberOfChannels:1});
        const chunk=new EncodedAudioChunk({type:'key',timestamp:0,data:new Uint8Array([128])});
        decoder.decode(chunk);decoder.decode(chunk);await decoder.flush();
        assert(outputs===2&&errors===0&&decoder.state==='configured','callback exception is not a codec failure');
        decoder.close();
    "#,
        &["owned output callback failure"],
    );
}

#[test]
fn closed_encoder_input_is_a_type_error_before_encoder_state_validation() {
    check(
        r#"
        const encoder=new AudioEncoder({output:()=>{},error:()=>{}});
        const audio=tone();audio.close();
        for(const configured of [false,true]) {
            if(configured)encoder.configure({codec:'opus',sampleRate:48000,numberOfChannels:1});
            let failure;try{encoder.encode(audio);}catch(error){failure=error;}
            assert(failure instanceof TypeError&&encoder.encodeQueueSize===0,'detached argument validity');
        }
        encoder.close();
    "#,
    );
}

#[test]
fn lifecycle_methods_validate_state_synchronously_except_flush_promise() {
    check(
        r#"
        for(const Ctor of [AudioDecoder,AudioEncoder]) {
            const codec=new Ctor({output:()=>{},error:()=>{}});
            assert(codec.state==='unconfigured','initial state');
            let error;try{await codec.flush();}catch(e){error=e;}
            assert(error.name==='InvalidStateError','unconfigured flush');
            codec.reset();codec.close();assert(codec.state==='closed','closed state');
            for(const operation of [()=>codec.reset(),()=>codec.close(),
                ()=>codec.configure({codec:'opus',sampleRate:48000,numberOfChannels:1})]) {
                error=undefined;try{operation();}catch(e){error=e;}
                assert(error.name==='InvalidStateError','closed method');
            }
            error=undefined;try{await codec.flush();}catch(e){error=e;}
            assert(error.name==='InvalidStateError','closed flush rejects');
        }
    "#,
    );
}

#[test]
fn close_aborts_every_flush_with_no_error_callback() {
    check(
        r#"
        let outputs=0,errors=0;
        const encoder=new AudioEncoder({output:()=>outputs++,error:()=>errors++});
        encoder.configure({codec:'opus',sampleRate:48000,numberOfChannels:1});encoder.encode(tone());
        const first=encoder.flush(),second=encoder.flush();encoder.close();
        const failures=await Promise.all([first.catch(error=>error),second.catch(error=>error)]);
        assert(failures.every(error=>error.name==='AbortError'),'all flushes aborted');
        assert(encoder.state==='closed'&&encoder.encodeQueueSize===0&&outputs===0&&errors===0,
            'close cancels all work without a fatal callback');
    "#,
    );
}

#[test]
fn reset_during_output_cancels_the_rest_of_one_native_result_batch() {
    check(
        r#"
        let outputs=0,errors=0;
        const encoder=new AudioEncoder({output:packet=>{outputs++;encoder.reset();},error:()=>errors++});
        encoder.configure({codec:'opus',sampleRate:48000,numberOfChannels:1});encoder.encode(tone(9600));
        let failure;try{await encoder.flush();}catch(error){failure=error;}
        assert(outputs===1&&errors===0&&failure.name==='AbortError','callback reset stops batch');
        assert(encoder.state==='unconfigured'&&encoder.encodeQueueSize===0,'generation reset');encoder.close();
    "#,
    );
}

#[test]
fn output_can_configure_new_work_without_mutating_prior_acceptance() {
    check(
        r#"
        const outputs=[];
        const decoder=new AudioDecoder({output:audio=>{
            outputs.push(audio);
            if(outputs.length===1) {
                decoder.configure({codec:'pcm-s16',sampleRate:22050,numberOfChannels:1});
                decoder.decode(new EncodedAudioChunk({type:'key',timestamp:22,data:new Int16Array([123])}));
            }
        },error:error=>{throw error;}});
        decoder.configure({codec:'pcm-u8',sampleRate:8000,numberOfChannels:1});
        decoder.decode(new EncodedAudioChunk({type:'key',timestamp:11,data:new Uint8Array([128])}));
        await decoder.flush();await decoder.flush();
        assert(outputs.length===2&&outputs[0].format==='u8'&&outputs[1].format==='s16','queued reconfigure');
        assert(outputs[0].sampleRate===8000&&outputs[1].sampleRate===22050,'output configurations');
        assert(outputs[0].timestamp===11&&outputs[1].timestamp===22,'accepted timestamps');
        for(const audio of outputs)audio.close();decoder.close();
    "#,
    );
}

#[test]
fn dequeue_is_trusted_and_respects_event_handler_registration_order() {
    check(
        r#"
        const codec=new AudioDecoder({output:audio=>audio.close(),error:error=>{throw error;}});
        const order=[];
        codec.addEventListener('dequeue',event=>{order.push('first');
            assert(event.isTrusted&&event.target===codec&&event.currentTarget===codec,'native event identity');
            assert(!event.bubbles&&!event.cancelable,'simple event');});
        codec.ondequeue=function(event){assert(this===codec,'handler receiver');order.push('obsolete');};
        codec.addEventListener('dequeue',()=>order.push('last'));
        codec.ondequeue=()=>order.push('replacement');
        codec.configure({codec:'pcm-u8',sampleRate:8000,numberOfChannels:1});
        codec.decode(new EncodedAudioChunk({type:'key',timestamp:0,data:new Uint8Array([128])}));
        await codec.flush();
        assert(order.join(',')==='first,replacement,last','handler replaces in its existing position');
        codec.ondequeue=null;codec.ondequeue=()=>order.push('new');order.length=0;
        codec.decode(new EncodedAudioChunk({type:'key',timestamp:0,data:new Uint8Array([128])}));await codec.flush();
        assert(order.join(',')==='first,last,new','null assignment releases listener position');codec.close();
    "#,
    );
}

#[test]
fn queue_resource_limits_reject_before_acceptance_and_do_not_close_codec() {
    check(
        r#"
        const decoder=new AudioDecoder({output:audio=>audio.close(),error:error=>{throw error;}});
        decoder.configure({codec:'pcm-u8',sampleRate:8000,numberOfChannels:1});
        const chunk=new EncodedAudioChunk({type:'key',timestamp:0,data:new Uint8Array([128])});
        for(let i=0;i<63;i++)decoder.decode(chunk);
        let failure;try{decoder.decode(chunk);}catch(error){failure=error;}
        assert(failure.name==='QuotaExceededError'&&decoder.decodeQueueSize===63,'bounded synchronous queue');
        assert(decoder.state==='configured','quota is not fatal');decoder.reset();
        decoder.configure({codec:'pcm-u8',sampleRate:8000,numberOfChannels:1});decoder.decode(chunk);await decoder.flush();
        assert(decoder.decodeQueueSize===0,'reusable after clearing queue');decoder.close();
    "#,
    );
}

#[test]
fn accepted_encoder_input_survives_close_and_transfer_of_author_resource() {
    check(
        r#"
        const packets=[];
        const encoder=new AudioEncoder({output:chunk=>packets.push(chunk),error:error=>{throw error;}});
        encoder.configure({codec:'opus',sampleRate:48000,numberOfChannels:1});
        const audio=tone();encoder.encode(audio);
        const transferred=structuredClone(audio,{transfer:[audio]});transferred.close();
        await encoder.flush();assert(packets.length>0,'encoder owns accepted PCM');encoder.close();
    "#,
    );
}
