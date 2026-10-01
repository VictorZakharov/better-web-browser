use super::*;

fn buffer_contract(code: &str, expected: &str) {
    let (_, outcome) = execute_html(&format!("<body><script>{code}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, [format!("log: {expected}")]);
}

#[test]
fn audio_buffer_dictionary_converts_in_lexical_order_with_one_get_per_member() {
    buffer_contract(
        r#"
        const reads=[],options={};
        for(const [name,value] of [['length',8.9],['numberOfChannels',4294967298],['sampleRate',8000.0003]])
            Object.defineProperty(options,name,{get(){reads.push('get:'+name);return {
                valueOf(){reads.push('number:'+name);return value;}
            };}});
        const buffer=new AudioBuffer(options);
        const expected='get:length,number:length,get:numberOfChannels,number:numberOfChannels,get:sampleRate,number:sampleRate';
        if(reads.join(',')!==expected) throw Error('buffer dictionary order '+reads);
        if(buffer.length!==8||buffer.numberOfChannels!==2||buffer.sampleRate!==Math.fround(8000.0003)||
            buffer.duration!==8/Math.fround(8000.0003)) throw Error('converted buffer metadata');
        console.log('buffer dictionary order passed');
        "#,
        "buffer dictionary order passed",
    );
}

#[test]
fn missing_required_members_and_primitive_dictionaries_throw_type_error() {
    buffer_contract(
        r#"
        for(const options of [undefined,null,{},1,true,'options',Symbol(),
            {length:8},{sampleRate:8000},{length:undefined,sampleRate:8000}]) {
            let error;
            try{new AudioBuffer(options);}catch(caught){error=caught;}
            if(error?.name!=='TypeError') throw Error('required buffer dictionary '+error);
        }
        let error;
        try{new AudioBuffer();}catch(caught){error=caught;}
        if(error?.name!=='TypeError') throw Error('missing options argument');
        const callable=()=>{};callable.length=8;
        Object.defineProperty(callable,'length',{value:8});callable.sampleRate=8000;
        if(new AudioBuffer(callable).numberOfChannels!==1) throw Error('callable dictionary');
        console.log('buffer required dictionary passed');
        "#,
        "buffer required dictionary passed",
    );
}

#[test]
fn unsigned_buffer_fields_wrap_and_truncate_before_nominal_validation() {
    buffer_contract(
        r#"
        const buffer=new AudioBuffer({length:4294967304,numberOfChannels:2.9,sampleRate:8000});
        if(buffer.length!==8||buffer.numberOfChannels!==2) throw Error('unsigned conversion');
        for(const [member,value] of [['length',0],['length',-1],['length',NaN],
            ['numberOfChannels',0],['numberOfChannels',33],['sampleRate',0],
            ['sampleRate',-1],['sampleRate',7999],['sampleRate',192001]]) {
            let error;
            try{new AudioBuffer({length:8,sampleRate:8000,[member]:value});}catch(caught){error=caught;}
            if(error?.name!=='NotSupportedError') throw Error('nominal range '+member+':'+error);
        }
        for(const member of ['length','numberOfChannels','sampleRate']) for(const value of [1n,Symbol()]) {
            let error;
            try{new AudioBuffer({length:8,sampleRate:8000,[member]:value});}catch(caught){error=caught;}
            if(error?.name!=='TypeError') throw Error('IDL conversion '+member);
        }
        console.log('buffer unsigned conversion passed');
        "#,
        "buffer unsigned conversion passed",
    );
}

#[test]
fn conversion_failures_stop_later_getters_but_semantic_failures_do_not() {
    buffer_contract(
        r#"
        let reads=0,error;
        try{new AudioBuffer({length:1n,get numberOfChannels(){reads++;return 1;}});}
        catch(caught){error=caught;}
        if(error?.name!=='TypeError'||reads!==0) throw Error('conversion short circuit');
        try{new AudioBuffer({length:0,get sampleRate(){reads++;return 8000;}});}
        catch(caught){error=caught;}
        if(error?.name!=='NotSupportedError'||reads!==1) throw Error('semantic validation before conversion');
        const marker={};
        try{new AudioBuffer({get length(){throw marker;},sampleRate:8000});}
        catch(caught){if(caught!==marker) throw Error('author exception was replaced');}
        console.log('buffer conversion boundaries passed');
        "#,
        "buffer conversion boundaries passed",
    );
}

#[test]
fn create_buffer_converts_arguments_left_to_right_before_semantic_validation() {
    buffer_contract(
        r#"
        const context=new OfflineAudioContext(1,128,8000),reads=[];
        const value=(name,number)=>({valueOf(){reads.push(name);return number;}});
        const buffer=context.createBuffer(value('channels',4294967298),value('length',8.9),value('rate',8000.0003));
        if(reads.join(',')!=='channels,length,rate'||buffer.numberOfChannels!==2||buffer.length!==8||
            buffer.sampleRate!==Math.fround(8000.0003)) throw Error('factory conversion');
        reads.length=0;
        let error;
        try{context.createBuffer(value('channels',0),value('length',8),value('rate',8000));}
        catch(caught){error=caught;}
        if(error?.name!=='NotSupportedError'||reads.join(',')!=='channels,length,rate')
            throw Error('factory validated before argument conversion');
        console.log('buffer factory conversion passed');
        "#,
        "buffer factory conversion passed",
    );
}

#[test]
fn create_buffer_checks_receiver_and_arity_before_author_conversions() {
    buffer_contract(
        r#"
        const context=new OfflineAudioContext(1,128,8000);
        const marker={valueOf(){throw Error('conversion ran before invocation check');}};
        for(const operation of [()=>context.createBuffer(),()=>context.createBuffer(marker),
            ()=>context.createBuffer(marker,8),()=>BaseAudioContext.prototype.createBuffer.call({},marker,8,8000),
            ()=>context.createBuffer(1n,8,8000),()=>context.createBuffer(1,8n,8000),
            ()=>context.createBuffer(1,8,8000n)]) {
            let error;try{operation();}catch(caught){error=caught;}
            if(error?.name!=='TypeError') throw Error('factory invocation '+error);
        }
        console.log('buffer factory invocation passed');
        "#,
        "buffer factory invocation passed",
    );
}
