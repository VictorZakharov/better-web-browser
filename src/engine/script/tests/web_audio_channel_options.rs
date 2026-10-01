use super::*;

#[test]
fn routing_members_are_enumerable_inherited_web_idl_properties() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context=new OfflineAudioContext(1,128,8000),node=context.createGain();
        for(const name of ['channelCount','channelCountMode','channelInterpretation']) {
            const descriptor=Object.getOwnPropertyDescriptor(AudioNode.prototype,name);
            if(!descriptor?.enumerable||!descriptor.configurable||
                typeof descriptor.get!=='function'||typeof descriptor.set!=='function'||
                Object.hasOwn(node,name)) throw Error('routing attribute '+name);
            for(const operation of [()=>descriptor.get.call({}),
                ()=>descriptor.set.call({},node[name])]) {
                let error;try{operation();}catch(caught){error=caught;}
                if(error?.name!=='TypeError') throw Error('routing receiver '+name);
            }
        }
        for(const name of ['connect','disconnect']) {
            const descriptor=Object.getOwnPropertyDescriptor(AudioNode.prototype,name);
            if(!descriptor.enumerable||!descriptor.configurable||!descriptor.writable||
                typeof descriptor.value!=='function') throw Error('routing operation '+name);
        }
        if(node.connect.length!==1||node.disconnect.length!==0)
            throw Error('routing operation required argument count');
        console.log('routing descriptors passed');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: routing descriptors passed"]);
}

#[test]
fn audio_node_channel_properties_are_inherited_branded_accessors() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const gain = context.createGain();
        for (const property of ['channelCount', 'channelCountMode', 'channelInterpretation']) {
            const descriptor = Object.getOwnPropertyDescriptor(AudioNode.prototype, property);
            if (!descriptor || typeof descriptor.get !== 'function' ||
                typeof descriptor.set !== 'function' ||
                Object.hasOwn(gain, property))
                throw Error('channel property is not an inherited accessor: ' + property);
            for (const receiver of [{}, AudioNode.prototype, context, null]) {
                let getter, setter;
                try { descriptor.get.call(receiver); } catch (error) { getter = error.name; }
                try { descriptor.set.call(receiver, 1); } catch (error) { setter = error.name; }
                if (getter !== 'TypeError' || setter !== 'TypeError')
                    throw Error('channel property brand check: ' + property);
            }
        }
        gain.channelCount = 4;
        gain.channelCountMode = 'explicit';
        gain.channelInterpretation = 'discrete';
        if (gain.channelCount !== 4 || gain.channelCountMode !== 'explicit' ||
            gain.channelInterpretation !== 'discrete')
            throw Error('channel setters did not retain state');
        console.log('channel accessors passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: channel accessors passed"]);
}

#[test]
fn generic_nodes_and_oscillators_accept_common_channel_options() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const shared = {channelCount: 4, channelCountMode: 'explicit',
            channelInterpretation: 'discrete'};
        const nodes = [new GainNode(context, shared), new DelayNode(context, shared),
            new AnalyserNode(context, shared), new BiquadFilterNode(context, shared),
            new IIRFilterNode(context, {...shared, feedforward: [1], feedback: [1]}),
            new WaveShaperNode(context, shared), new OscillatorNode(context, shared)];
        for (const node of nodes) {
            if (node.channelCount !== 4 || node.channelCountMode !== 'explicit' ||
                node.channelInterpretation !== 'discrete')
                throw Error(node.constructor.name + ' ignored inherited options');
            node.channelCountMode = 'max';
            node.channelCount = 32;
            node.channelInterpretation = 'speakers';
            if (node.channelCount !== 32 || node.channelCountMode !== 'max' ||
                node.channelInterpretation !== 'speakers')
                throw Error(node.constructor.name + ' ignored inherited setters');
        }
        console.log('common channel options passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: common channel options passed"]);
}

#[test]
fn unsigned_channel_count_conversion_matches_web_idl() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const node = context.createGain();
        const accepted = [[1.9,1], ['4',4], [4294967298,2], [-4294967295,1], [true,1]];
        for (const [value, expected] of accepted) {
            node.channelCount = value;
            if (node.channelCount !== expected) throw Error('unsigned setter conversion');
            if (new GainNode(context, {channelCount: value}).channelCount !== expected)
                throw Error('unsigned dictionary conversion');
        }
        for (const value of [0, 33, -1, NaN, Infinity, null, undefined, false]) {
            let name;
            try { node.channelCount = value; } catch (error) { name = error.name; }
            if (name !== 'NotSupportedError' || node.channelCount !== 1)
                throw Error('invalid converted channel count changed state');
        }
        for (const value of [1n, Symbol('count')]) {
            let name;
            try { node.channelCount = value; } catch (error) { name = error.name; }
            if (name !== 'TypeError') throw Error('ToNumber must reject BigInt/Symbol');
        }
        console.log('unsigned channel count passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: unsigned channel count passed"]);
}

#[test]
fn channel_enum_attributes_ignore_unknown_strings_but_dictionaries_reject_them() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const node = context.createGain();
        let calls = 0;
        node.channelCountMode = {toString() { calls++; return 'explicit'; }};
        node.channelInterpretation = {toString() { calls++; return 'discrete'; }};
        if (calls !== 2 || node.channelCountMode !== 'explicit' ||
            node.channelInterpretation !== 'discrete') throw Error('enum conversion count');
        for (const value of ['invalid', '', undefined, null, 1n]) {
            for (const property of ['channelCountMode', 'channelInterpretation']) {
                node[property] = value;
                if (value !== undefined) {
                    let name;
                    try { new GainNode(context, {[property]: value}); }
                    catch (error) { name = error.name; }
                    if (name !== 'TypeError') throw Error('invalid dictionary enum accepted');
                }
            }
        }
        for (const property of ['channelCountMode', 'channelInterpretation']) {
            let name;
            try { node[property] = Symbol('enum'); } catch (error) { name = error.name; }
            if (name !== 'TypeError') throw Error('ToString accepted a Symbol');
            const sentinel = new Error('sentinel');
            let caught;
            try { node[property] = {toString() {throw sentinel;}}; }
            catch (error) { caught = error; }
            if (caught !== sentinel) throw Error('enum swallowed conversion error');
        }
        if (node.channelCountMode !== 'explicit' || node.channelInterpretation !== 'discrete')
            throw Error('failed conversion corrupted channel state');
        console.log('channel enums passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: channel enums passed"]);
}

#[test]
fn standalone_source_dictionaries_do_not_read_unknown_channel_members() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const options = {};
        for (const property of ['channelCount', 'channelCountMode', 'channelInterpretation'])
            Object.defineProperty(options, property, {get() {
                throw Error('unknown source dictionary member read');
            }});
        for (const Type of [ConstantSourceNode, AudioBufferSourceNode]) {
            const source = new Type(context, options);
            if (source.channelCount !== 2 || source.channelCountMode !== 'max' ||
                source.channelInterpretation !== 'speakers') throw Error('source defaults');
            source.channelCount = 4;
            source.channelCountMode = 'explicit';
            source.channelInterpretation = 'discrete';
            if (source.channelCount !== 4 || source.channelCountMode !== 'explicit' ||
                source.channelInterpretation !== 'discrete') throw Error('inherited source setters');
        }
        console.log('standalone source options passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: standalone source options passed"]);
}

#[test]
fn inherited_dictionary_members_are_read_and_converted_in_order() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const events = [];
        const options = Object.create({
            get channelCount() { events.push('count'); return {valueOf() {
                events.push('count conversion'); return 2;
            }}; },
            get channelCountMode() { events.push('mode'); return {toString() {
                events.push('mode conversion'); return 'explicit';
            }}; },
            get channelInterpretation() { events.push('interpretation'); return {toString() {
                events.push('interpretation conversion'); return 'discrete';
            }}; }
        });
        Object.defineProperty(options, 'gain', {get() { events.push('gain'); return 0.5; }});
        const node = new GainNode(context, options);
        if (events.join(',') !== 'count,count conversion,mode,mode conversion,' +
            'interpretation,interpretation conversion,gain' || node.gain.value !== 0.5)
            throw Error('dictionary conversion order: ' + events.join(','));
        console.log('channel dictionary order passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: channel dictionary order passed"]);
}

#[test]
fn dictionary_conversion_stops_at_a_throwing_inherited_member() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        for (const member of ['channelCount', 'channelCountMode', 'channelInterpretation']) {
            const events = [];
            const sentinel = new Error('sentinel');
            const options = {};
            for (const property of ['channelCount', 'channelCountMode', 'channelInterpretation', 'gain'])
                Object.defineProperty(options, property, {get() {
                    events.push(property);
                    if (property === member) throw sentinel;
                    return undefined;
                }});
            let error;
            try { new GainNode(context, options); } catch (caught) { error = caught; }
            const expected = ['channelCount', 'channelCountMode', 'channelInterpretation']
                .slice(0, ['channelCount', 'channelCountMode', 'channelInterpretation'].indexOf(member) + 1);
            if (error !== sentinel || events.join(',') !== expected.join(','))
                throw Error('constructor swallowed a getter error or continued conversion');
        }
        for (const options of [1, 'options', true, 1n, Symbol('options')]) {
            let name;
            try { new GainNode(context, options); } catch (error) { name = error.name; }
            if (name !== 'TypeError') throw Error('primitive dictionary accepted');
        }
        if (new GainNode(context, null).channelCount !== 2 ||
            new GainNode(context, undefined).channelCount !== 2)
            throw Error('null/undefined dictionary must use defaults');
        console.log('channel conversion failure passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: channel conversion failure passed"]);
}

#[test]
fn source_channel_options_do_not_override_a_source_output_format() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(4, 128, 8000);
        context.destination.channelInterpretation = 'discrete';
        const source = new ConstantSourceNode(context, {offset: 0.5,
            channelCount: 4, channelCountMode: 'explicit'});
        const oscillator = new OscillatorNode(context, {frequency: 0, channelCount: 4});
        source.connect(context.destination); oscillator.connect(context.destination);
        source.start(); oscillator.start();
        context.startRendering().then(result => {
            for (let channel = 0; channel < 4; ++channel)
                if (!result.getChannelData(channel).every(sample =>
                    sample === (channel === 0 ? 0.5 : 0)))
                    throw Error('source channelCount changed its fixed mono output');
            console.log('source output format passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: source output format passed"]);
}
