use super::*;

fn feedback(code: &str, expected: &str) {
    let (_, outcome) = execute_html(&format!("<body><script>{code}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, [format!("log: {expected}")]);
}

#[test]
fn two_feedback_delays_read_the_previous_quantum_before_either_writer() {
    feedback(
        r#"
        const context=new OfflineAudioContext(1,768,8000);
        const buffer=context.createBuffer(1,1,8000);buffer.getChannelData(0)[0]=1;
        const source=new AudioBufferSourceNode(context,{buffer});
        const first=context.createDelay(.01),second=context.createDelay(.01);
        const gain=new GainNode(context,{gain:.5});
        source.connect(first);first.connect(second).connect(gain).connect(first);
        first.connect(context.destination);second.connect(context.destination);source.start();
        context.startRendering().then(buffer=> {
            const expected=new Map([[128,1],[256,1],[384,.5],[512,.5],[640,.25]]);
            for(let i=0;i<768;i++)
                if(buffer.getChannelData(0)[i] !== (expected.get(i)||0))
                    throw Error('reader/writer order '+i+':'+buffer.getChannelData(0)[i]);
            console.log('multiple delay scheduling passed');
        });
        "#,
        "multiple delay scheduling passed",
    );
}

#[test]
fn feedback_delay_can_break_an_audio_param_cycle_through_a_different_node() {
    feedback(
        r#"
        const context=new OfflineAudioContext(1,640,8000);
        const carrier=new ConstantSourceNode(context,{offset:1});
        const gain=new GainNode(context,{gain:.25}),delay=context.createDelay(.01);
        carrier.connect(gain);gain.connect(delay).connect(gain.gain);
        gain.connect(context.destination);carrier.start();
        context.startRendering().then(buffer=> {
            for(let i=0;i<640;i++) {
                const expected=.25*(Math.floor(i/128)+1);
                if(buffer.getChannelData(0)[i]!==expected) throw Error('delayed control cycle '+i);
            }
            console.log('delayed parameter cycle passed');
        });
        "#,
        "delayed parameter cycle passed",
    );
}

#[test]
fn delay_time_self_modulation_remains_a_residual_control_cycle() {
    feedback(
        r#"
        const context=new OfflineAudioContext(1,512,8000);
        const source=new ConstantSourceNode(context,{offset:.5}),delay=context.createDelay(.01);
        source.connect(delay);delay.connect(delay.delayTime);delay.connect(context.destination);
        source.start();
        context.startRendering().then(buffer=> {
            if(!buffer.getChannelData(0).every(value=>value===0))
                throw Error('delay reader recursively modulated itself');
            console.log('delay residual control passed');
        });
        "#,
        "delay residual control passed",
    );
}

#[test]
fn disconnecting_a_cycle_invalidates_the_cached_plan_at_the_next_quantum() {
    feedback(
        r#"
        const context=new OfflineAudioContext(1,512,8000);
        const source=new ConstantSourceNode(context,{offset:.5});
        const first=context.createGain(),second=context.createGain();
        source.connect(first).connect(second).connect(first);second.connect(context.destination);
        source.start();
        context.suspend(256/8000).then(()=>{second.disconnect(first);return context.resume();});
        context.startRendering().then(buffer=> {
            for(let i=0;i<512;i++)
                if(buffer.getChannelData(0)[i] !== (i<256?0:.5)) throw Error('stale muted plan '+i);
            console.log('cycle removal invalidation passed');
        });
        "#,
        "cycle removal invalidation passed",
    );
}

#[test]
fn adding_a_cycle_preserves_an_independent_route_and_does_not_replay_its_source() {
    feedback(
        r#"
        const context=new OfflineAudioContext(1,512,8000);
        const source=new ConstantSourceNode(context,{offset:.25});
        const first=context.createGain(),second=context.createGain();
        source.connect(first).connect(second).connect(context.destination);
        source.connect(context.destination);source.start();
        context.suspend(256/8000).then(()=>{second.connect(first);return context.resume();});
        context.startRendering().then(buffer=> {
            for(let i=0;i<512;i++)
                if(buffer.getChannelData(0)[i] !== (i<256?.5:.25)) throw Error('stale acyclic plan '+i);
            console.log('cycle addition invalidation passed');
        });
        "#,
        "cycle addition invalidation passed",
    );
}

#[test]
fn disconnected_feedback_history_is_advanced_before_later_destination_connection() {
    feedback(
        r#"
        const context=new OfflineAudioContext(1,512,8000);
        const buffer=context.createBuffer(1,1,8000);buffer.getChannelData(0)[0]=1;
        const source=new AudioBufferSourceNode(context,{buffer}),delay=context.createDelay(.01);
        const gain=new GainNode(context,{gain:.5});
        source.connect(delay).connect(gain).connect(delay);source.start();
        context.suspend(256/8000).then(()=>{delay.connect(context.destination);return context.resume();});
        context.startRendering().then(buffer=> {
            for(let i=0;i<512;i++) {
                const expected=i===256?.5:i===384?.25:0;
                if(buffer.getChannelData(0)[i]!==expected) throw Error('disconnected feedback '+i);
            }
            console.log('disconnected feedback history passed');
        });
        "#,
        "disconnected feedback history passed",
    );
}
