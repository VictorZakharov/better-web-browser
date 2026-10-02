//! Configuration replacements must not silently discard accepted partial PCM.
use super::check;

#[test]
fn replacement_encoder_drains_old_partial_frame_under_old_metadata() {
    check(
        r#"
        const outputs=[];
        const encoder=new AudioEncoder({output:(packet,info)=>outputs.push({packet,info}),error:error=>{throw error;}});
        encoder.configure({codec:'opus',sampleRate:48000,numberOfChannels:1,opus:{format:'ogg'}});
        const first=tone(17,1,48000,100000);encoder.encode(first);first.close();
        encoder.configure({codec:'opus',sampleRate:24000,numberOfChannels:2,opus:{format:'ogg'}});
        const second=tone(480,2,24000,200000);encoder.encode(second);second.close();await encoder.flush();
        const configs=outputs.filter(output=>output.info.decoderConfig).map(output=>output.info.decoderConfig);
        assert(configs.length===2,'one description per native configuration');
        assert(configs[0].sampleRate===48000&&configs[0].numberOfChannels===1,'old input is not relabeled');
        assert(configs[1].sampleRate===24000&&configs[1].numberOfChannels===2,'new configuration metadata');
        assert(outputs[0].packet.timestamp<100000&&outputs[0].packet.timestamp>90000,'old presentation time');
        const next=outputs.find(output=>output.info.decoderConfig?.sampleRate===24000);
        assert(next.packet.timestamp<200000&&next.packet.timestamp>190000,'new presentation time');encoder.close();
    "#,
    );
}

#[test]
fn repeated_flushes_resolve_in_order_and_emit_tail_only_once() {
    check(
        r#"
        const outputs=[],order=[];
        const encoder=new AudioEncoder({output:chunk=>outputs.push(chunk),error:error=>{throw error;}});
        encoder.configure({codec:'opus',sampleRate:48000,numberOfChannels:1});encoder.encode(tone(17));
        const first=encoder.flush().then(()=>order.push('first'));
        const second=encoder.flush().then(()=>order.push('second'));
        const third=encoder.flush().then(()=>order.push('third'));
        await Promise.all([first,second,third]);
        assert(order.join(',')==='first,second,third'&&outputs.length===1,'ordered drain with no duplicate tail');
        encoder.close();
    "#,
    );
}

#[test]
fn reset_while_reconfiguration_drains_suppresses_new_configuration_output() {
    check(
        r#"
        let outputs=0;
        const encoder=new AudioEncoder({output:()=>{outputs++;encoder.reset();},error:error=>{throw error;}});
        encoder.configure({codec:'opus',sampleRate:48000,numberOfChannels:1});encoder.encode(tone(7));
        encoder.configure({codec:'opus',sampleRate:24000,numberOfChannels:1});encoder.encode(tone(480,1,24000));
        let failure;try{await encoder.flush();}catch(error){failure=error;}
        assert(outputs===1&&failure.name==='AbortError'&&encoder.state==='unconfigured','cancelled replacement');
        encoder.close();
    "#,
    );
}

#[test]
fn changed_encoder_dimensions_fail_asynchronously_without_consuming_author_data() {
    check(
        r#"
        for(const audio of [tone(960,2),tone(480,1,24000)]) {
            const errors=[];
            const encoder=new AudioEncoder({output:()=>{throw Error('mismatch output');},error:error=>errors.push(error)});
            encoder.configure({codec:'opus',sampleRate:48000,numberOfChannels:1});encoder.encode(audio);
            assert(encoder.encodeQueueSize===1&&audio.numberOfFrames>0,'snapshot accepted');
            let failure;try{await encoder.flush();}catch(error){failure=error;}
            assert(failure.name==='EncodingError'&&errors[0]===failure&&encoder.state==='closed','dimension failure');
            assert(audio.numberOfFrames>0,'author still owns mismatched resource');audio.close();
        }
    "#,
    );
}

#[test]
fn support_query_resolves_on_a_later_task_after_dictionary_snapshot() {
    check(
        r#"
        const config={codec:'opus',sampleRate:48000,numberOfChannels:1};let resolved=false;
        const promise=AudioEncoder.isConfigSupported(config).then(result=>{resolved=true;return result;});
        config.codec='changed';await Promise.resolve();
        assert(!resolved,'support result is a task, not an already resolved promise');
        const result=await promise;assert(result.supported&&result.config.codec==='opus','synchronous dictionary snapshot');
    "#,
    );
}

#[test]
fn every_extended_opus_duration_is_visible_as_one_packet_per_full_input() {
    check(
        r#"
        for(const duration of [7500,12500,17500,30000,65000,120000]) {
            const packets=[];
            const encoder=new AudioEncoder({output:chunk=>packets.push(chunk),error:error=>{throw error;}});
            encoder.configure({codec:'opus',sampleRate:48000,numberOfChannels:1,opus:{frameDuration:duration}});
            const audio=tone(48000*duration/1000000);encoder.encode(audio);audio.close();await encoder.flush();
            assert(packets.length>=1&&packets.every(packet=>packet.duration===duration),'packet duration '+duration);
            const decoder=new AudioDecoder({output:audio=>{
                assert(audio.numberOfFrames===48000*duration/1000000,'decoded complete multi-frame packet');audio.close();
            },error:error=>{throw error;}});
            decoder.configure({codec:'opus',sampleRate:48000,numberOfChannels:1});
            for(const packet of packets)decoder.decode(packet);await decoder.flush();encoder.close();decoder.close();
        }
    "#,
    );
}
