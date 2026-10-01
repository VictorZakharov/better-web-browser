use super::*;

fn resources(code: &str, expected: &str) {
    let dom = dom::parse_with_scripting("<body><script></script></body>", true);
    let node = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.com/#resources".into(),
        code: code.into(),
        node,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let mut console = initial.console;
    // These near-limit renders intentionally yield many times. Do not replace
    // their real admission checks with a shorter, cheaper output buffer.
    for _ in 0..128 {
        if !console.is_empty() {
            break;
        }
        let outcome = runtime.advance_time(std::time::Duration::ZERO, 64);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        console.extend(outcome.console);
    }
    assert_eq!(console, [format!("log: {expected}")]);
}

#[test]
fn wide_intermediate_graph_is_not_priced_as_a_mono_destination() {
    resources(
        r#"
        const context=new OfflineAudioContext(1,200000,8000);
        const buffer=context.createBuffer(32,1,8000);
        const source=new AudioBufferSourceNode(context,{buffer,loop:true});
        const gains=Array.from({length:10},()=>context.createGain());
        for(const gain of gains) source.connect(gain);
        gains[0].connect(context.destination);source.start();
        context.startRendering().then(()=>{throw Error('wide graph bypassed work limit');},error=> {
            if(error.name!=='NotSupportedError'||context.currentTime!==0||context.state!=='suspended')
                throw Error('pre-render resource rejection was not atomic');
            console.log('wide intermediate admission passed');
        });
        "#,
        "wide intermediate admission passed",
    );
}

#[test]
fn rejected_fanin_connection_rolls_back_ports_and_work_accounting() {
    resources(
        r#"
        const context=new OfflineAudioContext(1,200000,8000);
        const buffer=context.createBuffer(32,1,8000);buffer.getChannelData(0)[0]=.25;
        const source=new AudioBufferSourceNode(context,{buffer,loop:true});
        const gains=Array.from({length:8},()=>context.createGain());
        for(const gain of gains) source.connect(gain);
        gains[0].connect(context.destination);source.start();
        const result=context.startRendering();
        let error;
        try{gains[1].connect(context.destination);}catch(caught){error=caught;}
        if(error?.name!=='NotSupportedError') throw Error('fanin admission accepted');
        try{gains[1].disconnect(context.destination);throw Error('rejected edge retained');}
        catch(error){if(error.name!=='InvalidAccessError') throw error;}
        // Repeated attempts must not consume the aggregate edge allowance.
        for(let i=0;i<520;i++)
            try{gains[1].connect(context.destination);throw Error('repeat admission accepted');}
            catch(error){if(error.name!=='NotSupportedError') throw error;}
        result.then(buffer=> {
            if(!buffer.getChannelData(0).every(value=>value===.25)) throw Error('failed edge changed PCM');
            console.log('fanin rollback passed');
        });
        "#,
        "fanin rollback passed",
    );
}

#[test]
fn explicit_channel_growth_is_rejected_atomically_after_render_start() {
    resources(
        r#"
        const context=new OfflineAudioContext(1,1000000,8000);
        const source=new ConstantSourceNode(context,{offset:.25});
        const gain=new GainNode(context,{channelCount:1,channelCountMode:'explicit'});
        const other=new GainNode(context,{channelCount:30,channelCountMode:'explicit'});
        source.connect(gain).connect(context.destination);source.start();
        const result=context.startRendering();
        let rejected=false;
        try{gain.channelCount=32;}catch(error){rejected=error.name==='NotSupportedError';}
        if(!rejected||gain.channelCount!==1||gain.channelCountMode!=='explicit')
            throw Error('failed channel growth altered state');
        // The failed new width must not poison the cached graph plan.
        gain.channelCount=2;
        gain.channelCount=1;
        result.then(buffer=> {
            if(!buffer.getChannelData(0).every(value=>value===.25)) throw Error('growth rollback PCM');
            console.log('channel growth rollback passed');
        });
        "#,
        "channel growth rollback passed",
    );
}

#[test]
fn audio_param_fanin_is_included_in_per_quantum_work_limit() {
    resources(
        r#"
        const context=new OfflineAudioContext(1,128,8000);
        const buffer=context.createBuffer(32,128,8000);
        const gain=context.createGain(),carrier=context.createConstantSource();
        carrier.connect(gain).connect(context.destination);carrier.start();
        for(let i=0;i<70;i++) {
            const source=new AudioBufferSourceNode(context,{buffer});
            source.connect(gain.gain);source.start();
        }
        context.startRendering().then(()=>{throw Error('parameter work was omitted');},error=> {
            if(error.name!=='NotSupportedError'||context.currentTime!==0)
                throw Error('parameter work rejection');
            console.log('parameter fanin budget passed');
        });
        "#,
        "parameter fanin budget passed",
    );
}
