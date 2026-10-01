use super::*;

fn lifecycle(code: &str, expected: &str) {
    let (_, outcome) = execute_html(&format!("<body><script>{code}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, [format!("log: {expected}")]);
}

#[test]
fn disconnecting_in_a_callback_discards_its_produced_block() {
    lifecycle(
        r#"
        const context=new OfflineAudioContext(1,1024,8000);
        const processor=context.createScriptProcessor(256,0,1);
        let calls=0;
        processor.onaudioprocess=event=> {
            calls++;
            event.outputBuffer.getChannelData(0).fill(.75);
            processor.disconnect();
        };
        processor.connect(context.destination);
        context.startRendering().then(buffer=> {
            if(calls!==1||!buffer.getChannelData(0).every(value=>value===0))
                throw Error('disconnected callback was audible');
            console.log('processing disconnect passed');
        });
        "#,
        "processing disconnect passed",
    );
}

#[test]
fn disconnected_intervals_reset_block_position_and_do_not_replay_old_output() {
    lifecycle(
        r#"
        const context=new OfflineAudioContext(1,1280,8000);
        const processor=context.createScriptProcessor(256,0,1);
        let calls=0;
        processor.onaudioprocess=event=> {
            calls++;
            event.outputBuffer.getChannelData(0).fill(calls===1?.25:.5);
        };
        processor.connect(context.destination);
        context.suspend(384/8000).then(()=>{processor.disconnect();return context.resume();});
        context.suspend(640/8000).then(()=>{processor.connect(context.destination);return context.resume();});
        context.startRendering().then(buffer=> {
            const values=buffer.getChannelData(0);
            for(let i=0;i<values.length;i++) {
                const expected=i>=256&&i<384?.25:i>=896?.5:0;
                if(values[i]!==expected) throw Error('stale processing sample '+i+':'+values[i]);
            }
            if(calls!==3) throw Error('disconnected callback cadence '+calls);
            console.log('processing reconnection passed');
        });
        "#,
        "processing reconnection passed",
    );
}

#[test]
fn suspended_render_preserves_partial_processing_input_across_resume() {
    lifecycle(
        r#"
        const context=new OfflineAudioContext(1,640,8000);
        const source=context.createConstantSource(),processor=context.createScriptProcessor(256,1,1);
        source.offset.value=.25;
        let calls=0;
        processor.onaudioprocess=event=> {
            calls++;
            const input=event.inputBuffer.getChannelData(0);
            if(calls===1) for(let i=0;i<256;i++)
                if(input[i] !== (i<128?.25:.75)) throw Error('partial input lost '+i);
            event.outputBuffer.getChannelData(0).set(input);
        };
        source.connect(processor).connect(context.destination);source.start();
        context.suspend(128/8000).then(()=>{source.offset.value=.75;return context.resume();});
        context.startRendering().then(buffer=> {
            const values=buffer.getChannelData(0);
            for(let i=0;i<640;i++)
                if(values[i] !== (i<256?0:i<384?.25:.75)) throw Error('resumed block '+i);
            console.log('processing partial suspension passed');
        });
        "#,
        "processing partial suspension passed",
    );
}

#[test]
fn fixed_input_count_still_honors_discrete_or_speaker_interpretation() {
    lifecycle(
        r#"
        async function render(interpretation) {
            const context=new OfflineAudioContext(1,512,8000);
            const buffer=context.createBuffer(2,512,8000);
            buffer.getChannelData(0).fill(.25);buffer.getChannelData(1).fill(.75);
            const source=new AudioBufferSourceNode(context,{buffer});
            const processor=context.createScriptProcessor(256,1,1);
            processor.channelInterpretation=interpretation;
            processor.onaudioprocess=event=>
                event.outputBuffer.getChannelData(0).set(event.inputBuffer.getChannelData(0));
            source.connect(processor).connect(context.destination);source.start();
            const result=await context.startRendering();
            for(let i=0;i<512;i++) {
                const expected=i<256?0:interpretation==='speakers'?.5:.25;
                if(result.getChannelData(0)[i]!==expected) throw Error('input interpretation '+i);
            }
        }
        render('speakers').then(()=>render('discrete')).then(()=>
            console.log('processing input interpretation passed'));
        "#,
        "processing input interpretation passed",
    );
}

#[test]
fn processor_without_handler_returns_silence_without_reusing_previous_output() {
    lifecycle(
        r#"
        const context=new OfflineAudioContext(1,1024,8000);
        const processor=context.createScriptProcessor(256,0,1);
        let calls=0;
        processor.onaudioprocess=event=> {
            calls++;
            event.outputBuffer.getChannelData(0).fill(.25);
            processor.onaudioprocess=null;
        };
        processor.connect(context.destination);
        context.startRendering().then(buffer=> {
            for(let i=0;i<1024;i++)
                if(buffer.getChannelData(0)[i] !== (i>=256&&i<512?.25:0))
                    throw Error('handler removal replayed output '+i);
            if(calls!==1) throw Error('removed callback ran again');
            console.log('processing handler removal passed');
        });
        "#,
        "processing handler removal passed",
    );
}

#[test]
fn detached_callback_output_is_silence_without_stopping_the_context() {
    lifecycle(
        r#"
        const context=new OfflineAudioContext(1,768,8000);
        const processor=context.createScriptProcessor(256,0,1);
        let calls=0;
        processor.onaudioprocess=event=> {
            const output=event.outputBuffer.getChannelData(0);
            output.fill(.25);
            if(++calls===1) structuredClone(output.buffer,{transfer:[output.buffer]});
        };
        processor.connect(context.destination);
        context.startRendering().then(buffer=> {
            if(context.state!=='closed'||calls!==2) throw Error('detachment stopped rendering');
            for(let i=0;i<768;i++)
                if(buffer.getChannelData(0)[i] !== (i>=512?.25:0)) throw Error('detached output '+i);
            console.log('processing detached output passed');
        });
        "#,
        "processing detached output passed",
    );
}

#[test]
fn processor_in_a_residual_cycle_is_silent_and_does_not_dispatch_callbacks() {
    lifecycle(
        r#"
        const context=new OfflineAudioContext(1,768,8000);
        const processor=context.createScriptProcessor(256,1,1),gain=context.createGain();
        let calls=0;
        processor.onaudioprocess=()=>calls++;
        processor.connect(gain).connect(processor);processor.connect(context.destination);
        context.startRendering().then(buffer=> {
            if(calls!==0||!buffer.getChannelData(0).every(value=>value===0))
                throw Error('residual processor cycle was evaluated');
            console.log('processing cycle passed');
        });
        "#,
        "processing cycle passed",
    );
}

#[test]
fn browser_processing_events_are_trusted_but_author_created_events_are_not() {
    lifecycle(
        r#"
        const context=new OfflineAudioContext(1,512,8000);
        const processor=context.createScriptProcessor(256,0,1);
        let seen=0;
        processor.onaudioprocess=event=> {
            if(!event.isTrusted||event.target!==processor||event.currentTarget!==processor||
                event.bubbles||event.cancelable||event.composed||event.type!=='audioprocess')
                throw Error('native processing event metadata');
            const author=new AudioProcessingEvent('audioprocess',{
                playbackTime:event.playbackTime,inputBuffer:event.inputBuffer,outputBuffer:event.outputBuffer});
            if(author.isTrusted) throw Error('author could forge trusted event');
            seen++;
        };
        processor.connect(context.destination);
        context.startRendering().then(()=> {
            if(seen!==1) throw Error('trusted callback not exercised');
            console.log('processing event provenance passed');
        });
        "#,
        "processing event provenance passed",
    );
}

#[test]
fn processed_stereo_output_modulates_an_audio_param_after_mono_downmix() {
    lifecycle(
        r#"
        const context=new OfflineAudioContext(1,768,8000);
        const processor=context.createScriptProcessor(256,0,2);
        const carrier=new ConstantSourceNode(context,{offset:1}),gain=new GainNode(context,{gain:0});
        let calls=0;
        processor.onaudioprocess=event=> {
            calls++;
            event.outputBuffer.getChannelData(0).fill(.25);
            event.outputBuffer.getChannelData(1).fill(.75);
        };
        processor.connect(gain.gain);carrier.connect(gain).connect(context.destination);carrier.start();
        context.startRendering().then(buffer=> {
            if(calls!==2) throw Error('parameter-only processing did not dispatch');
            for(let i=0;i<768;i++)
                if(buffer.getChannelData(0)[i] !== (i<256?0:.5)) throw Error('processing param downmix '+i);
            console.log('processing parameter output passed');
        });
        "#,
        "processing parameter output passed",
    );
}

#[test]
fn processing_callbacks_continue_with_silent_input_after_the_source_ends() {
    lifecycle(
        r#"
        const context=new OfflineAudioContext(1,1024,8000);
        const buffer=context.createBuffer(1,128,8000);buffer.getChannelData(0).fill(.25);
        const source=new AudioBufferSourceNode(context,{buffer});
        const processor=context.createScriptProcessor(256,1,1);
        let calls=0,ended=0;source.onended=()=>ended++;
        processor.onaudioprocess=event=> {
            const input=event.inputBuffer.getChannelData(0);
            for(let i=0;i<256;i++) {
                const expected=calls===0&&i<128?.25:0;
                if(input[i]!==expected) throw Error('finished source input was replayed');
            }
            event.outputBuffer.getChannelData(0).fill(.5);calls++;
        };
        source.connect(processor).connect(context.destination);source.start();
        context.startRendering().then(result=> {
            if(calls!==3||ended!==1) throw Error('source/processor lifetime coupling');
            for(let i=0;i<result.length;i++)
                if(result.getChannelData(0)[i] !== (i<256?0:.5)) throw Error('post-source synthesis');
            console.log('processing source completion passed');
        });
        "#,
        "processing source completion passed",
    );
}
