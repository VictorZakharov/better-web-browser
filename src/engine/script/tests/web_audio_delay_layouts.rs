use super::*;

fn render_layout_change(code: &str, expected: &str) {
    let (_, outcome) = execute_html(&format!("<script>{code}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, [format!("log: {expected}")]);
}

#[test]
fn delay_channel_decrease_waits_for_wider_history_to_arrive_before_downmix() {
    render_layout_change(
        r#"
        const context=new OfflineAudioContext(2,640,8000);
        const buffer=context.createBuffer(2,128,8000);
        buffer.getChannelData(0).fill(.25); buffer.getChannelData(1).fill(.5);
        const stereo=new AudioBufferSourceNode(context,{buffer});
        const mono=new ConstantSourceNode(context,{offset:.75});
        const delay=new DelayNode(context,{delayTime:200/8000,channelInterpretation:'discrete'});
        stereo.connect(delay).connect(context.destination); stereo.start();
        context.suspend(128/8000).then(()=> {
            stereo.disconnect(delay); mono.connect(delay); mono.start(context.currentTime);
            return context.resume();
        });
        context.startRendering().then(result=> {
            const left=result.getChannelData(0), right=result.getChannelData(1);
            for(let i=0;i<640;i++) {
                const l=i<200?0:i<328?.25:.75;
                // At frame 384 the prevailing output quantum finally becomes
                // mono, so the destination's speaker matrix duplicates it.
                const r=i<200?0:i<328?.5:i<384?0:.75;
                if(Math.abs(left[i]-l)>1e-6 || Math.abs(right[i]-r)>1e-6)
                    throw Error('delayed layout decrease at '+i+': '+left[i]+','+right[i]);
            }
            console.log('delay layout decrease passed');
        });
        "#,
        "delay layout decrease passed",
    );
}

#[test]
fn delay_channel_increase_does_not_upmix_old_mono_until_stereo_arrives() {
    render_layout_change(
        r#"
        const context=new OfflineAudioContext(2,640,8000);
        const mono=new ConstantSourceNode(context,{offset:.25});
        const buffer=context.createBuffer(2,512,8000);
        buffer.getChannelData(0).fill(.5); buffer.getChannelData(1).fill(.75);
        const stereo=new AudioBufferSourceNode(context,{buffer});
        const delay=new DelayNode(context,{delayTime:200/8000,channelInterpretation:'discrete'});
        mono.connect(delay).connect(context.destination); mono.start();
        context.suspend(128/8000).then(()=> {
            mono.disconnect(delay); stereo.connect(delay); stereo.start(context.currentTime);
            return context.resume();
        });
        context.startRendering().then(result=> {
            const left=result.getChannelData(0),right=result.getChannelData(1);
            for(let i=0;i<640;i++) {
                const l=i<200?0:i<328?.25:.5;
                const r=i<200?0:i<256?.25:i<328?0:.75;
                if(Math.abs(left[i]-l)>1e-6 || Math.abs(right[i]-r)>1e-6)
                    throw Error('delayed layout increase at '+i);
            }
            console.log('delay layout increase passed');
        });
        "#,
        "delay layout increase passed",
    );
}

fn fractional_layout(interpretation: &str, right: f64, label: &str) {
    render_layout_change(
        &format!(
            r#"
            const context=new OfflineAudioContext(2,512,8000);
            const before=context.createBuffer(2,128,8000);
            before.getChannelData(1)[127]=.5;
            const after=context.createBuffer(1,1,8000); after.getChannelData(0)[0]=.75;
            const stereo=new AudioBufferSourceNode(context,{{buffer:before}});
            const mono=new AudioBufferSourceNode(context,{{buffer:after}});
            const delay=new DelayNode(context,{{delayTime:128.5/8000,
                channelInterpretation:'{interpretation}'}});
            stereo.connect(delay).connect(context.destination); stereo.start();
            context.suspend(128/8000).then(()=> {{
                stereo.disconnect(delay); mono.connect(delay); mono.start(context.currentTime);
                return context.resume();
            }});
            context.startRendering().then(result=> {{
                const left=result.getChannelData(0),right=result.getChannelData(1);
                for(let i=0;i<512;i++) {{
                    const l=i===256 || i===257?.375:0;
                    const r=i===255?.25:i===256?{right}:i===257?
                        ('{interpretation}'==='speakers'?.375:0):0;
                    if(Math.abs(left[i]-l)>1e-6 || Math.abs(right[i]-r)>1e-6)
                        throw Error('fractional historical matrix '+i+': '+left[i]+','+right[i]);
                }}
                console.log('{label}');
            }});
            "#
        ),
        label,
    );
}

#[test]
fn fractional_delay_maps_each_historical_discrete_bus_before_interpolating() {
    fractional_layout("discrete", 0.25, "fractional discrete layout passed");
}

#[test]
fn fractional_delay_maps_each_historical_speaker_bus_before_interpolating() {
    fractional_layout("speakers", 0.625, "fractional speaker layout passed");
}

#[test]
fn delay_parameter_changes_and_ring_wrap_keep_exact_samples() {
    render_layout_change(
        r#"
        const context=new OfflineAudioContext(1,1024,8000);
        const buffer=context.createBuffer(1,1024,8000);
        const input=buffer.getChannelData(0);
        for(let i=0;i<input.length;i++) input[i]=i/2048;
        const source=new AudioBufferSourceNode(context,{buffer});
        const delay=new DelayNode(context,{maxDelayTime:.001,delayTime:0});
        delay.delayTime.setValueAtTime(4/8000,256/8000);
        delay.delayTime.setValueAtTime(0,512/8000);
        source.connect(delay).connect(context.destination); source.start();
        context.startRendering().then(result=> {
            const pcm=result.getChannelData(0);
            for(let i=0;i<1024;i++) {
                const expected=input[i>=256 && i<512?i-4:i];
                if(Math.abs(pcm[i]-expected)>1e-6) throw Error('delay wrap/automation at '+i);
            }
            console.log('delay ring automation passed');
        });
        "#,
        "delay ring automation passed",
    );
}

#[test]
fn delay_growth_memory_failure_rejects_rendering_and_cancels_future_suspensions() {
    render_layout_change(
        r#"
        const context=new OfflineAudioContext(1,1024,48000);
        const delay=new DelayNode(context,{maxDelayTime:100});
        const buffer=context.createBuffer(2,128,48000);
        const source=new AudioBufferSourceNode(context,{buffer});
        source.connect(delay).connect(context.destination); source.start();
        const names=[];
        context.suspend(256/48000).catch(error=>names.push(error.name));
        context.startRendering().then(()=>{throw Error('over-budget delay growth admitted');},error=> {
            names.push(error.name);
            queueMicrotask(()=> {
                if(names.join(',')!=='NotSupportedError,NotSupportedError' || context.state!=='closed')
                    throw Error('growth failure did not retire offline lifecycle: '+names);
                console.log('delay growth failure passed');
            });
        });
        "#,
        "delay growth failure passed",
    );
}
