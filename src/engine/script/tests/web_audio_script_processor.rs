use super::*;

#[test]
fn script_processor_factory_has_real_fixed_layout_and_illegal_constructor() {
    let (_, outcome) = execute_html(
        r#"<script>
        const context = new OfflineAudioContext(1, 128, 8000);
        let illegal = false;
        try { new ScriptProcessorNode(context); } catch (e) { illegal = e.name === 'TypeError'; }
        if (!illegal) throw Error('ScriptProcessor constructor is not a public factory');
        const defaults = context.createScriptProcessor();
        if (defaults.bufferSize !== 2048 || defaults.channelCount !== 2 ||
            defaults.channelCountMode !== 'explicit' || defaults.channelInterpretation !== 'speakers' ||
            defaults.numberOfInputs !== 1 || defaults.numberOfOutputs !== 1 ||
            defaults.context !== context || !(defaults instanceof AudioNode))
            throw Error('factory metadata');
        for (const size of [256,512,1024,2048,4096,8192,16384]) {
            const node = context.createScriptProcessor(size, 1, 4);
            if (node.bufferSize !== size || node.channelCount !== 1) throw Error('explicit size');
            node.channelCount = 1; node.channelCountMode = 'explicit';
            node.channelInterpretation = 'discrete';
            for (const [property,value] of [['channelCount',2],['channelCountMode','max']]) {
                let name;
                try { node[property] = value; } catch (error) { name = error.name; }
                if (name !== 'NotSupportedError') throw Error('fixed processor layout');
            }
        }
        console.log('script processor factory passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: script processor factory passed"]);
}

#[test]
fn script_processor_factory_converts_unsigned_arguments_before_validation() {
    let (_, outcome) = execute_html(
        r#"<script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const events = [];
        const value = (name, number) => ({valueOf() {events.push(name); return number;}});
        const node = context.createScriptProcessor(value('size',4294967552),
            value('input',1.9),value('output','2'));
        if (events.join(',') !== 'size,input,output' || node.bufferSize !== 256 ||
            node.channelCount !== 1) throw Error('IDL argument order or truncation');
        const invalid = [
            [255,1,1], [257,1,1], [32768,1,1], [256,0,0],
            [256,33,1], [256,1,33], [256,-1,1], [256,1,-1]
        ];
        for (const args of invalid) {
            let name;
            try { context.createScriptProcessor(...args); } catch (error) {name = error.name;}
            if (name !== 'IndexSizeError') throw Error('bad factory arguments: ' + args);
        }
        for (const args of [[1n,1,1], [256,1n,1], [256,1,1n], [Symbol(),1,1]]) {
            let name;
            try { context.createScriptProcessor(...args); } catch (error) {name = error.name;}
            if (name !== 'TypeError') throw Error('numeric conversion accepted BigInt/Symbol');
        }
        let name;
        try { BaseAudioContext.prototype.createScriptProcessor.call({}); }
        catch (error) {name = error.name;}
        if (name !== 'TypeError') throw Error('factory accepted unbranded context');
        console.log('script processor conversion passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: script processor conversion passed"]);
}

#[test]
fn script_processor_buffer_reservations_are_bounded_and_failed_calls_are_atomic() {
    let (_, outcome) = execute_html(
        r#"<script>
        const context = new OfflineAudioContext(1, 128, 8000);
        // Each 32+32-channel node reserves 8 MiB, including callback acquisition.
        context.createScriptProcessor(16384,32,32);
        context.createScriptProcessor(16384,32,32);
        for (let i = 0; i < 300; i++) {
            let name;
            try { context.createScriptProcessor(256,1,1); } catch (e) {name=e.name;}
            if (name !== 'NotSupportedError') throw Error('aggregate processor memory');
        }
        for (let i = 0; i < 253; i++) context.createGain();
        let name;
        try { context.createGain(); } catch (e) {name=e.name;}
        if (name !== 'NotSupportedError') throw Error('failed calls consumed node slots');
        console.log('script processor memory passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: script processor memory passed"]);
}

#[test]
fn processor_handler_and_buffer_size_are_branded_prototype_accessors() {
    let (_, outcome) = execute_html(
        r#"<script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const node = context.createScriptProcessor(256,1,1);
        for (const property of ['bufferSize','onaudioprocess']) {
            const descriptor = Object.getOwnPropertyDescriptor(ScriptProcessorNode.prototype,property);
            if (!descriptor || !descriptor.enumerable || !descriptor.configurable ||
                Object.hasOwn(node,property)) throw Error('processor descriptors');
            let name;
            try { descriptor.get.call({}); } catch (e) {name=e.name;}
            if (name !== 'TypeError') throw Error('unbranded getter');
            if (descriptor.set) {
                try { descriptor.set.call({},()=>{}); } catch (e) {name=e.name;}
                if (name !== 'TypeError') throw Error('unbranded setter');
            }
        }
        const handler = () => {};
        node.onaudioprocess = handler;
        if (node.onaudioprocess !== handler) throw Error('handler identity');
        node.onaudioprocess = {};
        if (node.onaudioprocess !== null) throw Error('noncallable handler');
        console.log('script processor descriptors passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: script processor descriptors passed"]
    );
}

#[test]
fn audio_processing_event_converts_required_dictionary_members_once_in_order() {
    let (_, outcome) = execute_html(
        r#"<script>
        const buffer = new AudioBuffer({length:16,sampleRate:8000});
        const reads = [];
        const init = {};
        for (const [name,value] of [['bubbles',true],['cancelable',false],['composed',true],
            ['inputBuffer',buffer],['outputBuffer',buffer],['playbackTime',0.25]])
            Object.defineProperty(init,name,{get() {reads.push(name); return value;}});
        const event = new AudioProcessingEvent('audioprocess',init);
        if (!(event instanceof Event) || event.inputBuffer !== buffer ||
            event.outputBuffer !== buffer || event.playbackTime !== 0.25 ||
            !event.bubbles || event.cancelable || !event.composed || event.isTrusted ||
            reads.join(',') !== 'bubbles,cancelable,composed,inputBuffer,outputBuffer,playbackTime')
            throw Error('processing event dictionary or event flags');
        for (const property of ['inputBuffer','outputBuffer','playbackTime']) {
            const descriptor = Object.getOwnPropertyDescriptor(AudioProcessingEvent.prototype,property);
            if (!descriptor || descriptor.set || !descriptor.enumerable ||
                !descriptor.configurable || Object.hasOwn(event,property)) throw Error('event descriptors');
            let name;
            try { descriptor.get.call({}); } catch (error) {name=error.name;}
            if (name !== 'TypeError') throw Error('processing event getter brand');
        }
        console.log('processing event conversion passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: processing event conversion passed"]);
}

#[test]
fn audio_processing_event_rejects_missing_buffers_nonfinite_times_and_forged_buffers() {
    let (_, outcome) = execute_html(
        r#"<script>
        const buffer = new AudioBuffer({length:16,sampleRate:8000});
        const valid = {inputBuffer:buffer,outputBuffer:buffer,playbackTime:0};
        const bad = [undefined,null,{},1, {outputBuffer:buffer,playbackTime:0},
            {inputBuffer:buffer,playbackTime:0}, {inputBuffer:buffer,outputBuffer:buffer},
            {...valid,inputBuffer:Object.create(AudioBuffer.prototype)},
            {...valid,outputBuffer:null}, {...valid,playbackTime:NaN},
            {...valid,playbackTime:Infinity}, {...valid,playbackTime:1n}];
        for (const options of bad) {
            let name;
            try { new AudioProcessingEvent('audioprocess',options); } catch (e) {name=e.name;}
            if (name !== 'TypeError') throw Error('invalid event dictionary accepted');
        }
        const sentinel = new Error('getter');
        let caught;
        try { new AudioProcessingEvent('audioprocess', {get inputBuffer() {throw sentinel;},
            get outputBuffer() {throw Error('later getter visited');}}); }
        catch (error) {caught=error;}
        if (caught !== sentinel) throw Error('processing event swallowed getter');
        console.log('processing event validation passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: processing event validation passed"]);
}
