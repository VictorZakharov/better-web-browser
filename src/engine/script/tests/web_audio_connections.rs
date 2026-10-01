use super::*;

#[test]
fn connect_brand_checks_precede_conversion_and_param_overload_ignores_input_argument() {
    let (_, outcome) = execute_html(
        r#"<script>
        const context=new OfflineAudioContext(1,128,8000);
        const gain=context.createGain(), source=context.createConstantSource();
        const sentinel={valueOf(){throw Error('unbranded receiver invoked conversion');}};
        for(const receiver of [{},AudioNode.prototype,null,Object.create(GainNode.prototype)]) {
            let name;
            try {AudioNode.prototype.connect.call(receiver,gain,sentinel);} catch(error) {name=error.name;}
            if(name!=='TypeError') throw Error('connect receiver branding');
            try {AudioNode.prototype.disconnect.call(receiver,sentinel);} catch(error) {name=error.name;}
            if(name!=='TypeError') throw Error('disconnect receiver branding');
        }
        for(const destination of [undefined,null,{},Object.create(AudioNode.prototype),
            Object.create(AudioParam.prototype)]) {
            let name; try {source.connect(destination);} catch(error) {name=error.name;}
            if(name!=='TypeError') throw Error('connect destination branding');
        }
        if(source.connect(gain)!==gain || source.connect(gain.gain,0,Symbol())!==undefined)
            throw Error('connect overload return or ignored argument');
        source.disconnect(gain.gain,0,Symbol());
        console.log('audio connect overload brands passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: audio connect overload brands passed"]
    );
}

#[test]
fn connect_indices_use_unsigned_web_idl_conversion_and_failed_conversion_is_atomic() {
    let (_, outcome) = execute_html(
        r#"<script>
        const context=new OfflineAudioContext(1,128,8000);
        const source=context.createConstantSource(), merger=context.createChannelMerger(2);
        const reads=[];
        const output={valueOf(){reads.push('output'); return 4294967296;}};
        const input={valueOf(){reads.push('input'); return 1.9;}};
        source.connect(merger,output,input);
        if(reads.join(',')!=='output,input') throw Error('connect numeric order');
        for(const value of [1n,Symbol()]) {
            let name; try {source.connect(merger,value,0);} catch(error) {name=error.name;}
            if(name!=='TypeError') throw Error('output conversion');
            try {source.connect(merger,0,value);} catch(error) {name=error.name;}
            if(name!=='TypeError') throw Error('input conversion');
        }
        for(const args of [[1,0],[0,2],[-1,0],[0,-1]]) {
            let name; try {source.connect(merger,...args);} catch(error) {name=error.name;}
            if(name!=='IndexSizeError') throw Error('invalid port index');
        }
        merger.channelInterpretation='discrete'; merger.connect(context.destination);
        context.destination.channelInterpretation='discrete'; source.start();
        context.startRendering().then(buffer=> {
            if(!buffer.getChannelData(0).every(value=>value===0)) throw Error('failed connection changed input');
            console.log('audio connect conversion passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: audio connect conversion passed"]);
}

#[test]
fn disconnect_numeric_overload_accepts_convertible_primitives_and_objects() {
    let (_, outcome) = execute_html(
        r#"<script>
        const context=new OfflineAudioContext(1,128,8000);
        const source=context.createConstantSource(), gain=context.createGain();
        const convertible=[undefined,null,false,'0',NaN,Infinity,4294967296,
            {valueOf(){return 0;}}];
        for(const value of convertible) {
            source.connect(gain); source.disconnect(value);
            let name; try {source.disconnect(gain);} catch(error) {name=error.name;}
            if(name!=='InvalidAccessError') throw Error('numeric overload did not disconnect');
        }
        for(const value of [1n,Symbol()]) {
            let name; try {source.disconnect(value);} catch(error) {name=error.name;}
            if(name!=='TypeError') throw Error('disconnect ToNumber');
        }
        source.disconnect(); source.disconnect(0);
        console.log('audio disconnect numeric overload passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: audio disconnect numeric overload passed"]
    );
}

#[test]
fn disconnect_preserves_distinct_input_edges_and_port_specific_parameter_edges() {
    let (_, outcome) = execute_html(
        r#"<script>
        const context=new OfflineAudioContext(2,128,8000);
        context.destination.channelInterpretation='discrete';
        const source=new ConstantSourceNode(context,{offset:.25});
        const merger=context.createChannelMerger(2), gain=new GainNode(context,{gain:1});
        source.connect(merger,0,0); source.connect(merger,0,1);
        source.connect(gain.gain); source.disconnect(gain.gain,undefined);
        source.disconnect(merger,undefined,undefined);
        let name; try {source.disconnect(merger,0,0);} catch(e) {name=e.name;}
        if(name!=='InvalidAccessError') throw Error('selected edge was not removed');
        merger.connect(context.destination); source.start();
        context.startRendering().then(buffer=> {
            if(!buffer.getChannelData(0).every(value=>value===0) ||
                !buffer.getChannelData(1).every(value=>value===.25)) throw Error('disconnect removed wrong input');
            console.log('audio disconnect edge selection passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: audio disconnect edge selection passed"]
    );
}

#[test]
fn failed_disconnect_leaves_every_existing_edge_and_context_isolation_intact() {
    let (_, outcome) = execute_html(
        r#"<script>
        const context=new OfflineAudioContext(1,128,8000), foreign=new OfflineAudioContext(1,128,8000);
        const source=new ConstantSourceNode(context,{offset:.25}), gain=context.createGain();
        source.connect(gain).connect(context.destination);
        const failures=[
            [()=>source.disconnect(gain,1),'IndexSizeError'],
            [()=>source.disconnect(gain,0,1),'IndexSizeError'],
            [()=>source.disconnect(gain,1n),'TypeError'],
            [()=>source.disconnect(foreign.destination),'InvalidAccessError'],
            [()=>source.connect(foreign.destination),'InvalidAccessError']
        ];
        for(const [operation,expected] of failures) {
            let name; try {operation();} catch(error) {name=error.name;}
            if(name!==expected) throw Error('graph edit failure: '+name);
        }
        source.start(); context.startRendering().then(buffer=> {
            if(!buffer.getChannelData(0).every(value=>value===.25)) throw Error('failed edit mutated graph');
            console.log('audio graph edit atomicity passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: audio graph edit atomicity passed"]);
}
