use super::*;

#[test]
fn all_audio_enum_attributes_convert_strings_once_and_ignore_unknown_values() {
    let (_, outcome) = execute_html(
        r#"<script>
        const context=new OfflineAudioContext(1,128,8000);
        const gain=context.createGain(), oscillator=context.createOscillator();
        const filter=context.createBiquadFilter(), shaper=context.createWaveShaper();
        const panner=context.createPanner();
        const entries=[
            [gain,'channelCountMode','explicit'], [gain,'channelInterpretation','discrete'],
            [gain.gain,'automationRate','k-rate'], [oscillator,'type','triangle'],
            [filter,'type','highpass'], [shaper,'oversample','2x'],
            [panner,'panningModel','equalpower'], [panner,'distanceModel','exponential']
        ];
        for(const [node,property,valid] of entries) {
            let reads=0;
            node[property]={toString(){reads++; return valid;}};
            if(reads!==1 || node[property]!==valid) throw Error('enum conversion: '+property);
            for(const invalid of ['unknown','',null,undefined,{},false,2,1n]) {
                node[property]=invalid;
                if(node[property]!==valid) throw Error('unknown enum altered state: '+property);
            }
            let name;
            try {node[property]=Symbol();} catch(error) {name=error.name;}
            if(name!=='TypeError' || node[property]!==valid) throw Error('symbol enum: '+property);
            const sentinel=new Error('conversion'); let caught;
            try {node[property]={toString(){throw sentinel;}};} catch(error) {caught=error;}
            if(caught!==sentinel) throw Error('enum conversion swallowed author exception');
        }
        console.log('all audio enum attributes passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: all audio enum attributes passed"]);
}

#[test]
fn audio_constructor_enum_dictionaries_reject_unknown_values_without_consuming_slots() {
    let (_, outcome) = execute_html(
        r#"<script>
        const context=new OfflineAudioContext(1,128,8000);
        const constructors=[
            [GainNode,'channelCountMode'], [GainNode,'channelInterpretation'],
            [OscillatorNode,'type'],[BiquadFilterNode,'type'],[WaveShaperNode,'oversample'],
            [PannerNode,'panningModel'],[PannerNode,'distanceModel']
        ];
        for(let i=0;i<300;i++) {
            const [Type,property]=constructors[i%constructors.length];
            let name;
            try {new Type(context,{[property]:'unknown'});} catch(error) {name=error.name;}
            if(name!=='TypeError') throw Error('invalid dictionary enum: '+property);
        }
        for(let i=0;i<255;i++) context.createGain();
        console.log('audio enum dictionary admission passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: audio enum dictionary admission passed"]
    );
}

#[test]
fn valid_but_unsupported_enum_values_keep_semantic_exceptions_and_atomic_state() {
    let (_, outcome) = execute_html(
        r#"<script>
        const context=new OfflineAudioContext(1,128,8000);
        const oscillator=context.createOscillator(), panner=context.createPanner();
        const source=context.createBufferSource(), compressor=context.createDynamicsCompressor();
        for(const [operation,expected] of [
            [()=>oscillator.type='custom','InvalidStateError'],
            [()=>panner.panningModel='HRTF','NotSupportedError'],
            [()=>source.playbackRate.automationRate='a-rate','InvalidStateError'],
            [()=>compressor.attack.automationRate='a-rate','InvalidStateError']
        ]) {
            let name; try {operation();} catch(error) {name=error.name;}
            if(name!==expected) throw Error('semantic enum exception');
        }
        if(oscillator.type!=='sine' || panner.panningModel!=='equalpower' ||
            source.playbackRate.automationRate!=='k-rate' || compressor.attack.automationRate!=='k-rate')
            throw Error('failed enum assignment changed state');
        console.log('semantic audio enum constraints passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: semantic audio enum constraints passed"]
    );
}

#[test]
fn floating_audio_arguments_follow_to_number_and_reject_bigint_even_inside_objects() {
    let (_, outcome) = execute_html(
        r#"<script>
        const context=new OfflineAudioContext(1,128,8000), gain=context.createGain();
        const operations=[
            value=>gain.gain.value=value,
            value=>gain.gain.setValueAtTime(value,0),
            value=>gain.gain.setTargetAtTime(1,0,value),
            value=>new GainNode(context,{gain:value}),
            value=>new ConstantSourceNode(context,{offset:value}),
            value=>new DelayNode(context,{delayTime:value}),
            value=>new StereoPannerNode(context,{pan:value}),
            value=>new BiquadFilterNode(context,{Q:value}),
            value=>new AudioBufferSourceNode(context,{playbackRate:value})
        ];
        for(const operation of operations)
            for(const value of [1n,{valueOf(){return 1n;}},Symbol(),NaN,Infinity]) {
                let name; try {operation(value);} catch(error) {name=error.name;}
                if(name!=='TypeError') throw Error('non-Web-IDL floating conversion');
            }
        gain.gain.value={valueOf(){return '0.5';}};
        if(gain.gain.value!==.5) throw Error('valid ToNumber conversion');
        console.log('audio floating conversion passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: audio floating conversion passed"]);
}
