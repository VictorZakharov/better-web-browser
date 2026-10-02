//! Web IDL conversion must precede resource validation and transfer side effects.
use super::check;

#[test]
fn required_members_and_invalid_dimensions_never_detach_input() {
    check(
        r#"
        const buffer=new ArrayBuffer(16);
        const base={data:buffer,format:'f32',numberOfChannels:1,numberOfFrames:4,
            sampleRate:48000,timestamp:0,transfer:[buffer]};
        const fail=init=>{let error;try{new AudioData(init);}catch(e){error=e;}
            assert(error instanceof TypeError,'invalid dictionary');assert(buffer.byteLength===16,'no detachment');};
        for(const name of ['data','format','numberOfChannels','numberOfFrames','sampleRate','timestamp']) {
            const init={...base};delete init[name];fail(init);
        }
        for(const name of ['numberOfChannels','numberOfFrames','sampleRate']) {
            for(const value of [0,-1,NaN,Infinity,-Infinity])fail({...base,[name]:value});
        }
        for(const value of [4294967296,1e100,1n,Symbol()]) {
            fail({...base,numberOfChannels:value});fail({...base,numberOfFrames:value});
        }
        for(const timestamp of [NaN,Infinity,-Infinity,1e100,1n,Symbol()])fail({...base,timestamp});
        fail({...base,format:'float32'});fail({...base,data:new Uint8Array(1)});
        const audio=new AudioData(base);assert(buffer.byteLength===0,'valid conversion transfers');audio.close();
    "#,
    );
}

#[test]
fn initializer_getters_are_visited_once_in_lexicographic_order() {
    check(
        r#"
        const order=[],values={data:new Float32Array(2),format:'f32',numberOfChannels:1,
            numberOfFrames:2,sampleRate:48000,timestamp:-1,transfer:[]};
        const init={};for(const name of Object.keys(values).reverse())
            Object.defineProperty(init,name,{get(){order.push(name);return values[name];}});
        Object.defineProperty(init,'unknown',{get(){throw Error('not an IDL member');}});
        const audio=new AudioData(init);
        assert(order.join(',')==='data,format,numberOfChannels,numberOfFrames,sampleRate,timestamp,transfer',
            'dictionary visits');audio.close();
    "#,
    );
}

#[test]
fn copy_dictionary_getters_are_converted_before_closed_resource_validation() {
    check(
        r#"
        const audio=new AudioData({data:new Float32Array(4),format:'f32',numberOfChannels:1,
            numberOfFrames:4,sampleRate:8000,timestamp:0});
        const order=[],values={format:'f32-planar',frameCount:2,frameOffset:1,planeIndex:0};
        const options={};for(const name of Object.keys(values).reverse())
            Object.defineProperty(options,name,{get(){order.push(name);return values[name];}});
        assert(audio.allocationSize(options)===8,'allocation');
        assert(order.join(',')==='format,frameCount,frameOffset,planeIndex','copy dictionary ordering');
        order.length=0;audio.close();let error;
        try{audio.allocationSize(options);}catch(e){error=e;}
        assert(error.name==='InvalidStateError'&&order.length===4,'convert before closed validation');
        try{audio.allocationSize({planeIndex:-1});}catch(e){error=e;}
        assert(error instanceof TypeError,'IDL range failure precedes closed state');
    "#,
    );
}

#[test]
fn numeric_conversions_follow_enforce_range_and_float32_rate() {
    check(
        r#"
        const audio=new AudioData({data:new Uint8Array(3),format:'u8',numberOfChannels:1.9,
            numberOfFrames:3.9,sampleRate:48000.0001,timestamp:-1.9});
        assert(audio.numberOfChannels===1&&audio.numberOfFrames===3,'unsigned truncation');
        assert(audio.sampleRate===Math.fround(48000.0001)&&audio.timestamp===-1,'float and signed truncation');
        assert(audio.allocationSize({planeIndex:.9,frameOffset:1.9,frameCount:1.9})===1,'copy IDL truncation');
        const audioBytes=new Uint8Array(3);audio.copyTo(audioBytes,{planeIndex:0});audio.close();
        const chunk=new EncodedAudioChunk({data:audioBytes,type:'key',timestamp:2**64,duration:-1});
        assert(chunk.timestamp===0&&chunk.duration===Number(2n**64n-1n),'ordinary chunk integer wrapping');
    "#,
    );
}

#[test]
fn prototype_getters_and_methods_reject_unbranded_receivers() {
    check(
        r#"
        for(const member of ['format','sampleRate','numberOfFrames','numberOfChannels','duration','timestamp']) {
            const descriptor=Object.getOwnPropertyDescriptor(AudioData.prototype,member);
            assert(descriptor.enumerable&&descriptor.configurable,'IDL attributes');
            let error;try{descriptor.get.call({});}catch(e){error=e;}
            assert(error instanceof TypeError,'getter brand '+member);
        }
        for(const member of ['close','clone','allocationSize','copyTo']) {
            assert(Object.getOwnPropertyDescriptor(AudioData.prototype,member).enumerable,'IDL method');
            let error;try{AudioData.prototype[member].call({});}catch(e){error=e;}
            assert(error instanceof TypeError,'method brand '+member);
        }
        for(const member of ['type','timestamp','duration','byteLength']) {
            let error;try{Object.getOwnPropertyDescriptor(EncodedAudioChunk.prototype,member).get.call({});}
            catch(e){error=e;}assert(error instanceof TypeError,'chunk brand '+member);
        }
        assert(AudioData.length===1&&EncodedAudioChunk.length===1,'constructor arity');
        assert(AudioData.prototype.copyTo.length===2&&AudioData.prototype.allocationSize.length===1,'method arity');
        assert(Object.getOwnPropertyDescriptor(AudioData.prototype,Symbol.toStringTag).value==='AudioData','IDL tag');
        for(const name of ['__audioCloneBindings','audioDataStates','encodedAudioStates','audioDataToken'])
            assert(!(name in globalThis),'no privileged globals');
    "#,
    );
}
