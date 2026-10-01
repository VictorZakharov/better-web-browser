use super::verify;

#[test]
fn malformed_configuration_rejects_a_promise_and_unknown_mime_does_not_throw() {
    verify(
        r#"
        const cap=navigator.mediaCapabilities;
        const base={contentType:'audio/webm;codecs=opus'};
        const video={contentType:'video/webm;codecs=vp8',width:1,height:1,bitrate:1000,framerate:30};
        const cases=[undefined,null,1,'record',{}, {type:'record'}, {audio:base},
            {type:'file',audio:base}, {type:'media-source',audio:base}, {type:'RECORD',audio:base},
            {type:'record',audio:{}}, {type:'record',audio:{contentType:Symbol()}},
            {type:'record',audio:{contentType:'audio/webm'}},
            {type:'record',audio:{contentType:'audio/flac;codecs=flac'}},
            {type:'record',audio:{contentType:'audio/webm;codecs=opus,vorbis'}},
            {type:'record',audio:{contentType:'audio/webm;codecs=opus;note=ignored'}},
            {type:'record',audio:{contentType:'video/webm;codecs=vp8'}},
            {type:'record',video:{...video,contentType:'audio/webm;codecs=opus'}},
            {type:'record',video:{...video,framerate:0}},
            {type:'record',video:{...video,framerate:Infinity}},
            {type:'record',video:{...video,framerate:NaN}},
            {type:'record',video:{...video,width:undefined}},
            {type:'record',audio:{...base,channels:Symbol()}},
            {type:'record',audio:{...base,bitrate:1n}}
        ];
        const reject=config=>{
            const promise=cap.encodingInfo(config);
            if(!(promise instanceof Promise)) throw Error('must return a Promise');
            return promise.then(()=>false,error=>error.name==='TypeError');
        };
        Promise.all(cases.map(reject)).then(results=>{
            if(!results.every(Boolean)) throw Error('invalid encoding configuration accepted');
            return cap.encodingInfo({type:'record',audio:{contentType:'audio/notimplemented'}});
        }).then(report=>{
            if(report.supported) throw Error('unknown codec');
            document.querySelector('output').textContent='passed';
        });
    "#,
    );
}

#[test]
fn getter_and_conversion_order_is_recursive_and_ignores_decoding_only_dictionary() {
    verify(
        r#"
        const reads=[];
        const observed=(name,value)=>new Proxy(value,{get(target,member){
            reads.push(name+'.'+String(member)); return target[member];}});
        const convert=(name,result)=>({valueOf(){reads.push(name);return result;}});
        const string=(name,result)=>({toString(){reads.push(name);return result;}});
        const audio=observed('audio',{bitrate:convert('convert.bitrate',64000),
            channels:string('convert.channels','2'),contentType:string('convert.mime','audio/webm;codecs=opus'),
            samplerate:convert('convert.rate',48000)});
        const config=observed('configuration',{audio,type:string('convert.type','record')});
        const pending=navigator.mediaCapabilities.encodingInfo(config);
        const expected=['configuration.audio','audio.bitrate','convert.bitrate',
            'audio.channels','convert.channels','audio.contentType','convert.mime',
            'audio.samplerate','convert.rate','audio.spatialRendering','configuration.video',
            'configuration.type','convert.type'];
        if(reads.join('|')!==expected.join('|')) throw Error('recursive conversion '+reads.join('|'));
        pending.then(report=>{
            if(!report.supported || report.configuration.audio.bitrate!==64000 ||
                report.configuration.audio.samplerate!==48000 || report.configuration.audio.channels!=='2')
                throw Error('converted snapshot');
            document.querySelector('output').textContent='passed';
        });
    "#,
    );
}

#[test]
fn abrupt_getters_stop_conversion_and_preserve_the_original_exception() {
    verify(
        r#"
        const token={identity:'author exception'};
        const reads=[];
        const audio={get bitrate(){reads.push('bitrate');throw token;},
            get contentType(){throw Error('later conversion must not run');}};
        const config={audio,get video(){throw Error('later getter must not run');},
            get type(){throw Error('derived getter must not run');}};
        navigator.mediaCapabilities.encodingInfo(config).then(()=>{
            throw Error('conversion should reject');
        },error=>{
            if(error!==token || reads.join(',')!=='bitrate') throw Error('conversion exception changed');
            document.querySelector('output').textContent='passed';
        });
    "#,
    );
}

#[test]
fn unsigned_member_conversion_does_not_mutate_the_input_dictionary() {
    verify(
        r#"
        const audio={contentType:'audio/webm;codecs=opus',channels:2,
            samplerate:2**32+48000.8,bitrate:64000.9};
        navigator.mediaCapabilities.encodingInfo({type:'record',audio}).then(report=>{
            if(!report.supported || report.configuration.audio.samplerate!==48000 ||
                report.configuration.audio.bitrate!==64000 || report.configuration.audio.channels!=='2' ||
                audio.channels!==2 || audio.samplerate!==2**32+48000.8 || audio.bitrate!==64000.9)
                throw Error('unsigned conversion or author dictionary mutation');
            document.querySelector('output').textContent='passed';
        });
    "#,
    );
}

#[test]
fn recording_and_webrtc_optional_video_members_follow_standard_applicability() {
    verify(
        r#"
        const capabilities=navigator.mediaCapabilities;
        const video={contentType:'video/VP9',width:320,height:240,bitrate:1000,framerate:30};
        const audio={contentType:'audio/webm;codecs=opus'};
        const invalid=[];
        for(const type of ['record','webrtc']) {
            for(const [name,value] of [['colorGamut','srgb'],['hdrMetadataType','smpteSt2086'],
                ['transferFunction','srgb'],['spatialScalability',false]])
                invalid.push(capabilities.encodingInfo({type,video:{...video,[name]:value}}));
            invalid.push(capabilities.encodingInfo({type,audio:{...audio,spatialRendering:false}}));
        }
        invalid.push(capabilities.encodingInfo({type:'record',video:{...video,scalabilityMode:'L1T1'}}));
        invalid.push(capabilities.decodingInfo({type:'webrtc',video:{...video,scalabilityMode:'L1T1'}}));
        Promise.all(invalid.map(p=>p.then(()=>false,error=>error.name==='TypeError'))).then(results=>{
            if(!results.every(Boolean)) throw Error('invalid optional member applicability');
            return capabilities.encodingInfo({type:'webrtc',video:{...video,
                hasAlphaChannel:true,scalabilityMode:'L1T1'}});
        }).then(report=>{
            if(report.supported || report.configuration.video.scalabilityMode!=='L1T1')
                throw Error('valid WebRTC encoding shape must remain unsupported');
            document.querySelector('output').textContent='passed';
        });
    "#,
    );
}

#[test]
fn capability_methods_reject_foreign_receivers_without_reading_author_properties() {
    verify(
        r#"
        const cap=navigator.mediaCapabilities;
        const config={get audio(){throw Error('foreign receiver read configuration');}};
        const promises=[];
        for(const method of [cap.encodingInfo,cap.decodingInfo]) {
            for(const receiver of [null,undefined,{},Object.create(cap)])
                promises.push(method.call(receiver,config));
        }
        Promise.all(promises.map(p=>p.then(()=>false,error=>error.name==='TypeError'))).then(results=>{
            if(!results.every(Boolean)) throw Error('capability receiver');
            document.querySelector('output').textContent='passed';
        });
    "#,
    );
}
