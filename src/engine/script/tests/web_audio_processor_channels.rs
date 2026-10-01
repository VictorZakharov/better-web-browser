use super::*;

fn processor_script(code: &str, label: &str) {
    let (_, outcome) = execute_html(&format!("<script>{code}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, [format!("log: {label}")]);
}

fn wide_identity(node: &str) {
    processor_script(
        &format!(
            r#"
            const context=new OfflineAudioContext(8,256,8000);
            context.destination.channelInterpretation='discrete';
            const buffer=context.createBuffer(8,256,8000);
            for(let c=0;c<8;c++) buffer.getChannelData(c)[c]=.5;
            const source=new AudioBufferSourceNode(context,{{buffer}});
            const processor={node};
            processor.channelInterpretation='discrete';
            source.connect(processor).connect(context.destination); source.start();
            context.startRendering().then(result=> {{
                for(let c=0;c<8;c++) for(let i=0;i<256;i++)
                    if(Math.abs(result.getChannelData(c)[i]-(i===c?.5:0))>1e-6)
                        throw Error('processor lost wide lane '+c+':'+i);
                console.log('wide processor passed');
            }});
            "#
        ),
        "wide processor passed",
    );
}

#[test]
fn gain_keeps_all_eight_intermediate_channels() {
    wide_identity("new GainNode(context)");
}

#[test]
fn zero_delay_keeps_all_eight_intermediate_channels() {
    wide_identity("new DelayNode(context, {delayTime:0})");
}

#[test]
fn biquad_has_separate_histories_for_all_eight_channels() {
    wide_identity("new BiquadFilterNode(context, {type:'allpass',frequency:0})");
}

#[test]
fn iir_has_separate_histories_for_all_eight_channels() {
    wide_identity("new IIRFilterNode(context, {feedforward:[1],feedback:[1]})");
}

#[test]
fn waveshaper_preserves_wide_identity_pcm() {
    wide_identity("new WaveShaperNode(context, {curve:[-1,1]})");
}

#[test]
fn analyser_pass_through_is_not_limited_by_destination_width() {
    wide_identity("new AnalyserNode(context, {fftSize:32})");
}

#[test]
fn nonlinear_shaping_uses_configured_input_mix_before_processing() {
    processor_script(
        r#"
        const context=new OfflineAudioContext(2,256,8000);
        context.destination.channelInterpretation='discrete';
        const buffer=context.createBuffer(2,256,8000);
        buffer.getChannelData(0).fill(.25); buffer.getChannelData(1).fill(-.25);
        const source=new AudioBufferSourceNode(context,{buffer});
        const mono=new WaveShaperNode(context,{curve:[1,0,1],channelCount:1,
            channelCountMode:'explicit'});
        const stereo=new WaveShaperNode(context,{curve:[1,0,1]});
        const merge=context.createChannelMerger(2);
        source.connect(mono).connect(merge,0,0);
        source.connect(stereo).connect(merge,0,1);
        merge.connect(context.destination); source.start();
        context.startRendering().then(result=> {
            if(!result.getChannelData(0).every(v=>v===0) ||
                !result.getChannelData(1).every(v=>v===.25)) throw Error('nonlinear mix order');
            console.log('nonlinear input mix passed');
        });
        "#,
        "nonlinear input mix passed",
    );
}

#[test]
fn analyser_mono_speaker_mix_omits_lfe_even_with_discrete_passthrough() {
    processor_script(
        r#"
        const context=new OfflineAudioContext(6,256,8000);
        context.destination.channelInterpretation='discrete';
        const buffer=context.createBuffer(6,256,8000); buffer.getChannelData(3).fill(1);
        const source=new AudioBufferSourceNode(context,{buffer});
        const analyser=new AnalyserNode(context,{fftSize:32,channelCount:6,
            channelCountMode:'explicit',channelInterpretation:'discrete'});
        source.connect(analyser).connect(context.destination); source.start();
        context.startRendering().then(result=> {
            const data=new Float32Array(32); analyser.getFloatTimeDomainData(data);
            if(!data.every(v=>v===0) || !result.getChannelData(3).every(v=>v===1))
                throw Error('LFE analysis or pass-through interpretation');
            console.log('analyser LFE exclusion passed');
        });
        "#,
        "analyser LFE exclusion passed",
    );
}

#[test]
fn merger_downmixes_each_input_independently_and_empty_ports_keep_their_position() {
    processor_script(
        r#"
        const context=new OfflineAudioContext(4,256,8000);
        context.destination.channelInterpretation='discrete';
        const buffer=context.createBuffer(2,256,8000);
        buffer.getChannelData(0).fill(.25); buffer.getChannelData(1).fill(.75);
        const source=new AudioBufferSourceNode(context,{buffer});
        const merger=context.createChannelMerger(4);
        const mono=new ConstantSourceNode(context,{offset:-.25});
        source.connect(merger,0,1); mono.connect(merger,0,3);
        merger.connect(context.destination); source.start(); mono.start();
        context.startRendering().then(result=> {
            for(let c=0;c<4;c++)
                if(!result.getChannelData(c).every(v=>v===(c===1?.5:c===3?-.25:0)))
                    throw Error('merger input layout or empty port');
            console.log('merger independent inputs passed');
        });
        "#,
        "merger independent inputs passed",
    );
}

#[test]
fn analyser_channel_setters_change_passthrough_without_reusing_stale_width_plans() {
    processor_script(
        r#"
        const context=new OfflineAudioContext(2,512,8000);
        context.destination.channelInterpretation='discrete';
        const buffer=context.createBuffer(2,512,8000);
        buffer.getChannelData(0).fill(.25); buffer.getChannelData(1).fill(.75);
        const source=new AudioBufferSourceNode(context,{buffer});
        const analyser=new AnalyserNode(context,{fftSize:32});
        source.connect(analyser).connect(context.destination); source.start();
        context.suspend(256/8000).then(()=> {
            analyser.channelCount=1; analyser.channelCountMode='explicit';
            analyser.channelInterpretation='discrete'; return context.resume();
        });
        context.startRendering().then(result=> {
            for(let i=0;i<512;i++)
                if(result.getChannelData(0)[i]!==.25 ||
                    result.getChannelData(1)[i] !== (i<256?.75:0)) throw Error('stale analyser bus width');
            const data=new Float32Array(32); analyser.getFloatTimeDomainData(data);
            if(!data.every(v=>v===.25)) throw Error('analysis ignored configured mono input');
            console.log('analyser dynamic layout passed');
        });
        "#,
        "analyser dynamic layout passed",
    );
}
