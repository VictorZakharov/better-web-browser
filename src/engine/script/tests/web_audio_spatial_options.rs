use super::*;

fn options_contract(code: &str, expected: &str) {
    let (_, outcome) = execute_html(&format!("<body><script>{code}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, [format!("log: {expected}")]);
}

#[test]
fn panner_dictionary_reads_and_converts_each_member_once_in_inheritance_order() {
    options_contract(
        r#"
        const context=new OfflineAudioContext(2,128,8000), reads=[];
        const members={channelCount:1,channelCountMode:'explicit',channelInterpretation:'discrete',
            coneInnerAngle:90,coneOuterAngle:180,coneOuterGain:.5,distanceModel:'linear',
            maxDistance:20,orientationX:0,orientationY:1,orientationZ:2,panningModel:'equalpower',
            positionX:3,positionY:4,positionZ:5,refDistance:2,rolloffFactor:.25};
        const options={};
        for(const [name,value] of Object.entries(members))
            Object.defineProperty(options,name,{get(){
                reads.push('get:'+name);
                return {[Symbol.toPrimitive](hint){
                    reads.push(hint+':'+name); return value;
                }};
            }});
        const node=new PannerNode(context,options);
        const expected=[];
        for(const name of Object.keys(members)) {
            expected.push('get:'+name);
            expected.push((['channelCountMode','channelInterpretation','distanceModel','panningModel']
                .includes(name)?'string':'number')+':'+name);
        }
        if(reads.join('|')!==expected.join('|')) throw Error('dictionary order '+reads);
        for(const [name,value] of Object.entries(members)) {
            const actual=node[name] instanceof AudioParam?node[name].value:node[name];
            if(actual!==value) throw Error('converted value '+name+':'+actual);
        }
        console.log('spatial dictionary conversion passed');
        "#,
        "spatial dictionary conversion passed",
    );
}

#[test]
fn panner_conversion_failure_short_circuits_before_later_getters() {
    options_contract(
        r#"
        const context=new OfflineAudioContext(2,128,8000), reads=[];
        const options={
            get coneOuterGain(){reads.push('gain');return .5;},
            get distanceModel(){reads.push('model');return 'unsupported';},
            get maxDistance(){throw Error('later getter ran');}
        };
        let error;
        try {new PannerNode(context,options);} catch(caught){error=caught;}
        if(error?.name!=='TypeError'||reads.join(',')!=='gain,model')
            throw Error('conversion did not stop: '+error);
        const marker={};
        try {new PannerNode(context,{get coneInnerAngle(){throw marker;}});}
        catch(caught){if(caught!==marker) throw Error('author exception replaced');}
        console.log('spatial short circuit passed');
        "#,
        "spatial short circuit passed",
    );
}

#[test]
fn panner_semantic_rejection_occurs_only_after_dictionary_conversion() {
    options_contract(
        r#"
        const context=new OfflineAudioContext(2,128,8000);
        for(const [member,value,name] of [['coneOuterGain',2,'InvalidStateError'],
            ['maxDistance',0,'RangeError'],['refDistance',-1,'RangeError'],
            ['panningModel','HRTF','NotSupportedError'],['channelCount',3,'NotSupportedError']]) {
            let tailReads=0,error;
            const options={[member]:value,get rolloffFactor(){tailReads++;return .5;}};
            try{new PannerNode(context,options);}catch(caught){error=caught;}
            if(error?.name!==name||tailReads!==1)
                throw Error('semantic validation order '+member+':'+error);
        }
        // Failed constructors must not leave listener-dependent ghost nodes.
        for(let i=0;i<300;i++)
            try{new PannerNode(context,{panningModel:'HRTF'});}catch(error){
                if(error.name!=='NotSupportedError') throw error;
            }
        const node=context.createPanner();
        if(node.panningModel!=='equalpower') throw Error('node admission leaked');
        console.log('spatial semantic order passed');
        "#,
        "spatial semantic order passed",
    );
}

#[test]
fn spatial_float_conversion_rounds_before_initializing_audio_params() {
    options_contract(
        r#"
        const context=new OfflineAudioContext(2,128,8000);
        const value=1.00000003;
        const node=new PannerNode(context,{positionX:value,orientationZ:value});
        if(node.positionX.value!==Math.fround(value)||node.orientationZ.value!==Math.fround(value))
            throw Error('float conversion was skipped');
        for(const member of ['positionX','orientationZ','coneOuterGain','maxDistance']) {
            for(const value of [1n,Symbol(),Infinity,NaN]) {
                let error;
                try{new PannerNode(context,{[member]:value});}catch(caught){error=caught;}
                if(error?.name!=='TypeError') throw Error('non-number '+member+':'+error);
            }
        }
        console.log('spatial numeric conversion passed');
        "#,
        "spatial numeric conversion passed",
    );
}
