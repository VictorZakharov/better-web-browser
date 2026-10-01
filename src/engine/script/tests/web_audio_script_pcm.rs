use super::*;

fn rendered_script(code: &str, expected: &str) {
    let (_, outcome) = execute_html(&format!("<body><script>{code}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, [format!("log: {expected}")]);
}

#[test]
fn script_processor_transforms_actual_stereo_pcm_at_its_reported_playback_time() {
    rendered_script(
        r#"
        const context = new OfflineAudioContext(2,1024,8000);
        const buffer = context.createBuffer(2,512,8000);
        buffer.getChannelData(0)[0]=0.5; buffer.getChannelData(0)[255]=-0.5;
        buffer.getChannelData(1)[128]=0.75; buffer.getChannelData(1)[256]=0.25;
        const source = new AudioBufferSourceNode(context,{buffer});
        const processor = context.createScriptProcessor(256,2,2);
        const blocks = [];
        processor.onaudioprocess = event => {
            if (!(event instanceof AudioProcessingEvent) || event.target !== processor ||
                event.currentTarget !== processor || event.inputBuffer.length !== 256 ||
                event.outputBuffer.length !== 256 || event.inputBuffer.sampleRate !== 8000)
                throw Error('real processing event metadata');
            blocks.push(Math.round(event.playbackTime*8000));
            for (let c=0;c<2;c++) {
                const input=event.inputBuffer.getChannelData(1-c);
                const output=event.outputBuffer.getChannelData(c);
                for(let i=0;i<256;i++) output[i]=input[i]*0.5;
            }
        };
        source.connect(processor).connect(context.destination); source.start();
        context.startRendering().then(result => {
            if (blocks.join(',') !== '256,512,768') throw Error('callback cadence: '+blocks);
            for (let c=0;c<2;c++) for(let i=0;i<1024;i++) {
                const expected=i>=256 && i<768 ? buffer.getChannelData(1-c)[i-256]*0.5 : 0;
                if(result.getChannelData(c)[i] !== expected) throw Error('transformed PCM '+c+':'+i);
            }
            console.log('script stereo PCM passed');
        });
        "#,
        "script stereo PCM passed",
    );
}

#[test]
fn zero_input_processor_synthesizes_audio_without_a_fake_input_channel() {
    rendered_script(
        r#"
        const context=new OfflineAudioContext(2,768,8000);
        const processor=context.createScriptProcessor(256,0,2);
        let calls=0;
        processor.onaudioprocess=event => {
            calls++;
            if(event.inputBuffer.numberOfChannels !== 0 || event.inputBuffer.length !== 256 ||
                event.outputBuffer.numberOfChannels !== 2 || processor.channelCount !== 0)
                throw Error('zero-input layout');
            let name;
            try {event.inputBuffer.getChannelData(0);} catch(e) {name=e.name;}
            if(name !== 'IndexSizeError') throw Error('zero-input buffer manufactured samples');
            event.outputBuffer.getChannelData(0).fill(0.25);
            event.outputBuffer.getChannelData(1).fill(-0.5);
        };
        processor.connect(context.destination);
        context.startRendering().then(buffer=> {
            if(calls!==2) throw Error('synthesis callback count');
            for(let c=0;c<2;c++) for(let i=0;i<768;i++)
                if(buffer.getChannelData(c)[i] !== (i<256?0:c?-.5:.25))
                    throw Error('script synthesis PCM');
            console.log('script synthesis passed');
        });
        "#,
        "script synthesis passed",
    );
}

#[test]
fn zero_output_processor_receives_real_audio_without_an_audible_output() {
    rendered_script(
        r#"
        const context=new OfflineAudioContext(2,768,8000);
        const source=new ConstantSourceNode(context,{offset:.75});
        const processor=context.createScriptProcessor(256,2,0);
        let calls=0;
        processor.onaudioprocess=event => {
            calls++;
            if(event.outputBuffer.numberOfChannels!==0 || event.inputBuffer.numberOfChannels!==2)
                throw Error('analysis-only event layout');
            for(let c=0;c<2;c++)
                if(!event.inputBuffer.getChannelData(c).every(value=>value===.75))
                    throw Error('input speaker upmix');
        };
        source.connect(processor); source.start();
        context.startRendering().then(buffer=> {
            if(calls!==2 || !buffer.getChannelData(0).every(value=>value===0) ||
                !buffer.getChannelData(1).every(value=>value===0)) throw Error('analysis-only route');
            console.log('script analysis-only passed');
        });
        "#,
        "script analysis-only passed",
    );
}

#[test]
fn script_output_is_acquired_before_callback_microtasks_and_later_author_mutation() {
    rendered_script(
        r#"
        const context=new OfflineAudioContext(1,768,8000);
        const processor=context.createScriptProcessor(256,0,1);
        const retained=[];
        processor.onaudioprocess=event=> {
            const samples=event.outputBuffer.getChannelData(0);
            samples.fill(.25); retained.push(samples);
            queueMicrotask(()=>samples.fill(.75));
            setTimeout(()=>samples.fill(-1),0);
        };
        processor.connect(context.destination);
        context.startRendering().then(buffer=> {
            for(let i=0;i<buffer.length;i++)
                if(buffer.getChannelData(0)[i] !== (i<256?0:.25)) throw Error('late output mutation leaked');
            if(retained.length!==2 || retained.some(samples=>samples[0]===.25))
                throw Error('author mutation was not exercised');
            console.log('script output acquisition passed');
        });
        "#,
        "script output acquisition passed",
    );
}

#[test]
fn partial_final_quantum_uses_only_a_produced_block_and_preserves_length() {
    rendered_script(
        r#"
        const context=new OfflineAudioContext(1,409,8000);
        const processor=context.createScriptProcessor(256,0,1);
        let calls=0;
        processor.onaudioprocess=event=> {
            calls++;
            const output=event.outputBuffer.getChannelData(0);
            for(let i=0;i<output.length;i++) output[i]=i/256;
        };
        processor.connect(context.destination);
        context.startRendering().then(buffer=> {
            if(buffer.length!==409 || calls!==1) throw Error('partial block completion');
            for(let i=0;i<409;i++)
                if(buffer.getChannelData(0)[i] !== (i<256?0:(i-256)/256)) throw Error('partial PCM');
            console.log('script partial quantum passed');
        });
        "#,
        "script partial quantum passed",
    );
}

#[test]
fn fanout_pulls_processor_once_and_listener_handler_replacement_takes_effect() {
    rendered_script(
        r#"
        const context=new OfflineAudioContext(1,768,8000);
        const processor=context.createScriptProcessor(256,0,1);
        const first=context.createGain(), second=context.createGain();
        let calls=0, listenerCalls=0, obsolete=0;
        processor.onaudioprocess=()=>obsolete++;
        processor.onaudioprocess=event=>{calls++; event.outputBuffer.getChannelData(0).fill(.25);};
        processor.addEventListener('audioprocess',()=>listenerCalls++);
        processor.connect(first).connect(context.destination);
        processor.connect(second).connect(context.destination);
        processor.connect(first);
        context.startRendering().then(buffer=> {
            if(calls!==2 || listenerCalls!==2 || obsolete!==0) throw Error('duplicate rendering/listeners');
            for(let i=0;i<768;i++)
                if(buffer.getChannelData(0)[i] !== (i<256?0:.5)) throw Error('processor fanout');
            console.log('script fanout passed');
        });
        "#,
        "script fanout passed",
    );
}

#[test]
fn unconnected_processor_has_no_callbacks_then_starts_at_connection_time() {
    rendered_script(
        r#"
        const context=new OfflineAudioContext(1,768,8000);
        const processor=context.createScriptProcessor(256,0,1);
        let calls=0;
        processor.onaudioprocess=event=> {calls++; event.outputBuffer.getChannelData(0).fill(.5);};
        context.suspend(128/8000).then(()=> {
            if(calls!==0) throw Error('unconnected processor dispatched');
            processor.connect(context.destination); return context.resume();
        });
        context.startRendering().then(buffer=> {
            if(calls!==2) throw Error('late-connected callback cadence');
            for(let i=0;i<768;i++)
                if(buffer.getChannelData(0)[i] !== (i<384?0:.5)) throw Error('late connection PCM');
            console.log('script late connection passed');
        });
        "#,
        "script late connection passed",
    );
}

#[test]
fn callback_graph_edits_take_effect_on_the_next_quantum_without_reentrancy() {
    rendered_script(
        r#"
        const context=new OfflineAudioContext(1,768,8000);
        const processor=context.createScriptProcessor(256,0,1);
        let source;
        processor.onaudioprocess=event=> {
            event.outputBuffer.getChannelData(0).fill(.25);
            if(!source) {
                source=new ConstantSourceNode(context,{offset:.5});
                source.connect(context.destination); source.start(context.currentTime);
            }
        };
        processor.connect(context.destination);
        context.startRendering().then(buffer=> {
            for(let i=0;i<768;i++)
                if(buffer.getChannelData(0)[i] !== (i<256?0:.75)) throw Error('callback graph edit PCM');
            console.log('script graph edit passed');
        });
        "#,
        "script graph edit passed",
    );
}

#[test]
fn replacing_public_dispatch_and_timer_functions_cannot_hijack_processing() {
    rendered_script(
        r#"
        const context=new OfflineAudioContext(1,512,8000);
        const processor=context.createScriptProcessor(256,0,1);
        processor.onaudioprocess=event=>event.outputBuffer.getChannelData(0).fill(.25);
        processor.dispatchEvent=()=>{throw Error('author dispatch used by renderer');};
        globalThis.setTimeout=()=>{throw Error('author timer used by renderer');};
        processor.connect(context.destination);
        context.startRendering().then(buffer=> {
            if(buffer.getChannelData(0)[256]!==.25) throw Error('processing was intercepted');
            console.log('private processing dispatch passed');
        });
        "#,
        "private processing dispatch passed",
    );
}
