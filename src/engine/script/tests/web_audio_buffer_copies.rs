use super::*;

fn copies(code: &str, expected: &str) {
    let (_, outcome) = execute_html(&format!("<body><script>{code}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, [format!("log: {expected}")]);
}

#[test]
fn audio_buffer_methods_are_branded_and_require_their_nonoptional_arguments() {
    copies(
        r#"
        const buffer=new AudioBuffer({length:8,sampleRate:8000}),array=new Float32Array(8);
        const conversion={valueOf(){throw Error('unbranded conversion ran');}};
        for(const operation of [()=>buffer.getChannelData(),()=>buffer.copyFromChannel(array),
            ()=>buffer.copyToChannel(array),()=>AudioBuffer.prototype.getChannelData.call({},conversion),
            ()=>AudioBuffer.prototype.copyFromChannel.call({},array,conversion),
            ()=>AudioBuffer.prototype.copyToChannel.call({},array,conversion)]) {
            let error;try{operation();}catch(caught){error=caught;}
            if(error?.name!=='TypeError') throw Error('buffer invocation '+error);
        }
        for(const name of ['getChannelData','copyFromChannel','copyToChannel']) {
            const descriptor=Object.getOwnPropertyDescriptor(AudioBuffer.prototype,name);
            if(!descriptor.enumerable||!descriptor.configurable||!descriptor.writable)
                throw Error('buffer method descriptor '+name);
        }
        console.log('buffer method invocation passed');
        "#,
        "buffer method invocation passed",
    );
}

#[test]
fn get_channel_data_uses_unsigned_conversion_and_internal_metadata() {
    copies(
        r#"
        const buffer=new AudioBuffer({length:8,numberOfChannels:2,sampleRate:8000});
        const first=buffer.getChannelData(0),second=buffer.getChannelData(1);
        if(buffer.getChannelData(4294967297)!==second||buffer.getChannelData(1.9)!==second||
            buffer.getChannelData(NaN)!==first||buffer.getChannelData(undefined)!==first)
            throw Error('unsigned channel conversion');
        for(const value of [1n,Symbol(),{valueOf(){return 1n;}}]) {
            let error;try{buffer.getChannelData(value);}catch(caught){error=caught;}
            if(error?.name!=='TypeError') throw Error('channel ToNumber');
        }
        Object.defineProperty(buffer,'numberOfChannels',{get(){throw Error('public getter used');}});
        Object.defineProperty(buffer,'length',{get(){throw Error('public length used');}});
        if(buffer.getChannelData(1)!==second) throw Error('private channel identity');
        buffer.copyToChannel(new Float32Array([.25]),1);
        const output=new Float32Array(1);buffer.copyFromChannel(output,1);
        if(output[0]!==.25) throw Error('private buffer copy');
        console.log('buffer internal metadata passed');
        "#,
        "buffer internal metadata passed",
    );
}

#[test]
fn copy_methods_finish_argument_conversion_before_channel_index_validation() {
    copies(
        r#"
        const buffer=new AudioBuffer({length:8,sampleRate:8000}),array=new Float32Array(1);
        for(const name of ['copyFromChannel','copyToChannel']) {
            const reads=[];let error;
            try{buffer[name](array,{valueOf(){reads.push('channel');return 99;}},
                {valueOf(){reads.push('offset');return 0;}});}catch(caught){error=caught;}
            if(error?.name!=='IndexSizeError'||reads.join(',')!=='channel,offset')
                throw Error('copy argument order '+name+':'+reads);
            try{buffer[name](array,99,1n);}catch(caught){error=caught;}
            if(error?.name!=='TypeError') throw Error('conversion must precede range validation');
            try{buffer[name]({}, {valueOf(){throw Error('later channel converted');}});}
            catch(caught){error=caught;}
            if(error?.name!=='TypeError') throw Error('array conversion must run first');
        }
        console.log('buffer copy argument order passed');
        "#,
        "buffer copy argument order passed",
    );
}

#[test]
fn channel_copies_ignore_public_method_overrides_and_preserve_unwritten_ranges() {
    copies(
        r#"
        const buffer=new AudioBuffer({length:8,sampleRate:8000});
        const channel=buffer.getChannelData(0);channel.set([1,2,3,4,5,6,7,8]);
        buffer.getChannelData=()=>{throw Error('author method used by native copy');};
        const destination=new Float32Array([9,9,9,9,9]);
        buffer.copyFromChannel(destination,0,6.9);
        if(destination.join(',')!=='7,8,9,9,9') throw Error('destination tail overwritten');
        buffer.copyToChannel(new Float32Array([.25,.5,.75]),0,4294967303);
        if(channel.join(',')!=='1,2,3,4,5,6,7,0.25') throw Error('source clipping or offset wrap');
        const before=channel.slice();
        buffer.copyToChannel(new Float32Array([1]),0,-1);
        buffer.copyFromChannel(destination,0,8);
        if(channel.join(',')!==before.join(',')||destination.join(',')!=='7,8,9,9,9')
            throw Error('out-of-buffer offset changed data');
        console.log('buffer copy boundaries passed');
        "#,
        "buffer copy boundaries passed",
    );
}

#[test]
fn overlapping_views_use_snapshot_copy_semantics() {
    copies(
        r#"
        const buffer=new AudioBuffer({length:8,sampleRate:8000});
        const channel=buffer.getChannelData(0);channel.set([1,2,3,4,5,6,7,8]);
        buffer.copyToChannel(channel.subarray(0,6),0,2);
        if(channel.join(',')!=='1,2,1,2,3,4,5,6') throw Error('overlapping copyToChannel');
        channel.set([1,2,3,4,5,6,7,8]);
        buffer.copyFromChannel(channel.subarray(2),0,0);
        if(channel.join(',')!=='1,2,1,2,3,4,5,6') throw Error('overlapping copyFromChannel');
        console.log('buffer overlapping views passed');
        "#,
        "buffer overlapping views passed",
    );
}

#[test]
fn channel_copies_use_intrinsic_typed_array_storage_and_methods() {
    copies(
        r#"
        const buffer=new AudioBuffer({length:4,sampleRate:8000});
        const channel=buffer.getChannelData(0),source=new Float32Array([1,2,3]);
        const destination=new Float32Array(4);
        for(const array of [channel,source,destination]) {
            for(const name of ['length','buffer','byteOffset','set','subarray','constructor'])
                Object.defineProperty(array,name,{get(){throw Error('author property '+name);}});
        }
        const originalSet=Float32Array.prototype.set;
        Float32Array.prototype.set=()=>{throw Error('author set');};
        try {
            buffer.copyToChannel(source,0,1);
            buffer.copyFromChannel(destination,0);
        } finally { Float32Array.prototype.set=originalSet; }
        if(destination[0]!==0||destination[1]!==1||destination[2]!==2||destination[3]!==3)
            throw Error('intrinsic copy PCM');
        console.log('buffer intrinsic copies passed');
        "#,
        "buffer intrinsic copies passed",
    );
}

#[test]
fn detachment_during_copy_argument_conversion_leaves_other_samples_untouched() {
    copies(
        r#"
        const buffer=new AudioBuffer({length:4,sampleRate:8000});
        const channel=buffer.getChannelData(0);channel.set([1,2,3,4]);
        for(const name of ['copyFromChannel','copyToChannel']) {
            const array=new Float32Array([9,9]);
            buffer[name](array,{valueOf(){
                structuredClone(array.buffer,{transfer:[array.buffer]});return 0;
            }});
            if(channel.join(',')!=='1,2,3,4') throw Error('detached conversion changed PCM');
        }
        console.log('buffer conversion detachment passed');
        "#,
        "buffer conversion detachment passed",
    );
}

#[test]
fn copy_methods_reject_resizable_shared_and_forged_typed_arrays_but_allow_empty_views() {
    copies(
        r#"
        const buffer=new AudioBuffer({length:8,sampleRate:8000});
        const detached=new Float32Array(4);
        structuredClone(detached.buffer,{transfer:[detached.buffer]});
        const values=[new Float64Array(4),new Int32Array(4),
            Object.create(Float32Array.prototype),{[Symbol.toStringTag]:'Float32Array'}];
        if(typeof SharedArrayBuffer==='function') values.push(new Float32Array(new SharedArrayBuffer(16)));
        const resizable=new ArrayBuffer(16,{maxByteLength:32});
        if(resizable.resizable) values.push(new Float32Array(resizable));
        for(const value of values) for(const name of ['copyFromChannel','copyToChannel']) {
            let error;try{buffer[name](value,0);}catch(caught){error=caught;}
            if(error?.name!=='TypeError') throw Error('invalid array '+name+':'+error);
        }
        for(const empty of [new Float32Array(0),detached]) {
            buffer.copyFromChannel(empty,0);buffer.copyToChannel(empty,0);
        }
        if(!buffer.getChannelData(0).every(value=>value===0)) throw Error('rejection changed PCM');
        console.log('buffer typed array validation passed');
        "#,
        "buffer typed array validation passed",
    );
}
