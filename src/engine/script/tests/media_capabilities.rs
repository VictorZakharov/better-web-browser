use super::*;

#[test]
fn capability_audio_members_are_optional_and_out_of_backend_bounds_are_unsupported() {
    let (dom, outcome) = execute_html(
        r#"<output>pending</output><script>
        const base = { contentType:'audio/webm;codecs=vorbis' };
        const configurations = [
            base, {...base,channels:2,samplerate:44100},
            {...base,channels:'8'}, {...base,channels:'stereo'},
            {...base,channels:'5.1'}, {...base,samplerate:0},
            {...base,samplerate:7999}, {...base,samplerate:192001},
            {...base,spatialRendering:true}, {...base,samplerate:Infinity},
            {contentType:'audio/aac',channels:'1',samplerate:96000},
            {contentType:'audio/aac',channels:'1',samplerate:192000},
            {contentType:'audio/mpeg',channels:'2',samplerate:96000},
            {contentType:'audio/flac',channels:'2',samplerate:384000},
            {contentType:'audio/flac',channels:'1',samplerate:8000},
            {contentType:'audio/flac',channels:'2',samplerate:192000},
            {contentType:'audio/flac',channels:'1',samplerate:7999},
            {contentType:'audio/flac',channels:'2',samplerate:192001},
            {contentType:'audio/flac',channels:'1',samplerate:200001}
        ];
        Promise.all(configurations.map(audio => navigator.mediaCapabilities.decodingInfo({type:'file',audio})))
        .then(reports => {
            const expected = [true,true,false,false,false,false,false,false,false,false,true,false,false,false,
                true,true,false,false,false];
            for(let i=0;i<reports.length;i++) {
                if(reports[i].supported !== expected[i] || reports[i].smooth || reports[i].powerEfficient)
                    throw Error('bounded report '+i);
            }
            document.querySelector('output').textContent='passed';
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}

#[test]
fn media_source_aac_capabilities_use_the_native_fragmented_decoder_rate_contract() {
    let (dom, outcome) = execute_html(
        r#"<output>pending</output><script>
        const contentType='audio/mp4;codecs=mp4a.40.2';
        const configurations=[['file',192000],['media-source',192000],
            ['media-source',44100],['media-source',48000],['media-source',64000],['media-source',96000]];
        Promise.all([
            ...configurations.map(([type,samplerate])=>navigator.mediaCapabilities.decodingInfo({type,
                audio:{contentType,channels:2,samplerate}})),
            navigator.mediaCapabilities.decodingInfo({type:'media-source',audio:{contentType}})
        ]).then(reports=>{
            const expected=[true,false,true,true,false,false,true];
            for(let i=0;i<reports.length;i++) {
                if(reports[i].supported!==expected[i] || reports[i].smooth || reports[i].powerEfficient ||
                    reports[i].configuration.audio.contentType!==contentType) throw Error('mode rate '+i);
            }
            document.querySelector('output').textContent='passed';
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}

#[test]
fn invalid_track_mime_rejects_but_valid_unknown_formats_resolve_unsupported() {
    let (dom, outcome) = execute_html(
        r#"<output>pending</output><script>
        const video = {width:320,height:240,bitrate:500000,framerate:30};
        const invalid = [
            {type:'file',audio:{contentType:'not-mime'}},
            {type:'file',audio:{contentType:'video/mp4;codecs=avc1.42e01e'}},
            {type:'file',audio:{contentType:'audio/webm'}},
            {type:'file',audio:{contentType:'audio/webm;codecs=vorbis,opus'}},
            {type:'file',audio:{contentType:'audio/webm;note=ignored;codecs=vorbis'}},
            {type:'file',audio:{contentType:'audio/aac;codecs=mp4a.40.2'}},
            {type:'file',video:{...video,contentType:'audio/webm;codecs=vorbis'}},
            {type:'file',video:{...video,contentType:'video/mp4;codecs=mp4a.40.2'}},
            {type:'file',video:{...video,contentType:'application/ogg;codecs=vorbis'}},
            {type:'file',audio:{contentType:'application/ogg;codecs=theora'}}
        ];
        const rejection = config => navigator.mediaCapabilities.decodingInfo(config)
            .then(()=>false,error=>error.name==='TypeError');
        Promise.all(invalid.map(rejection)).then(results=>{
            if(!results.every(Boolean)) throw Error('MIME validity');
            return navigator.mediaCapabilities.decodingInfo({type:'file',audio:{contentType:'audio/unknown'}});
        }).then(report=>{
            if(report.supported || report.smooth || report.powerEfficient) throw Error('unknown format');
            document.querySelector('output').textContent='passed';
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}

#[test]
fn capability_results_snapshot_known_dictionary_members_and_settle_after_microtasks() {
    let (dom, outcome) = execute_html(
        r#"<output>pending</output><script>
        const capabilities = navigator.mediaCapabilities;
        if(capabilities !== navigator.mediaCapabilities) throw Error('SameObject');
        let reads=0, settled=false;
        const audio = {get contentType(){reads++;return 'audio/webm;codecs=vorbis';},
            channels:2,samplerate:44100,ignored:'discard'};
        const pending = capabilities.decodingInfo({type:'file',audio});
        audio.channels='8';audio.samplerate=0;
        Promise.resolve().then(()=>{if(settled) throw Error('settled in caller microtasks');});
        pending.then(report=>{
            settled=true;
            if(!report.supported || reads!==1 || report.configuration.audio.channels!=='2' ||
                report.configuration.audio.samplerate!==44100 || 'ignored' in report.configuration.audio)
                throw Error('dictionary snapshot');
            document.querySelector('output').textContent='passed';
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}

#[test]
fn bounded_video_queries_do_not_claim_hdr_alpha_or_unmeasured_smoothness() {
    let (dom, outcome) = execute_html(
        r#"<output>pending</output><script>
        const base={contentType:'video/mp4;codecs=avc1.42e01e',width:320,height:240,bitrate:500000,framerate:30};
        Promise.all([base,{...base,width:8193},{...base,width:8192,height:8192},
            {...base,colorGamut:'p3'},{...base,hasAlphaChannel:true},{...base,transferFunction:'pq'}]
            .map(video=>navigator.mediaCapabilities.decodingInfo({type:'file',video})))
        .then(reports=>{
            if(!reports[0].supported || reports.slice(1).some(report=>report.supported) ||
                reports.some(report=>report.smooth || report.powerEfficient)) throw Error('video overclaim');
            return navigator.mediaCapabilities.decodingInfo({type:'file',video:{...base,colorGamut:'invalid'}})
                .then(()=>false,error=>error.name==='TypeError');
        }).then(rejected=>{
            if(!rejected) throw Error('enum');
            document.querySelector('output').textContent='passed';
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}

#[test]
fn capability_dictionary_getters_follow_inherited_recursive_conversion_order() {
    let (dom, outcome) = execute_html(
        r#"<output>pending</output><script>
        const reads=[];
        const observed=(name,values)=>new Proxy(values,{get(target,member){
            reads.push(name+'.'+member);return target[member];
        }});
        const audio={contentType:'audio/webm;codecs=vorbis',channels:2,samplerate:44100};
        const video={contentType:'video/mp4;codecs=avc1.42e01e',width:320,height:240,bitrate:500000,framerate:30};
        const key=observed('key',{
            audio:observed('key.audio',{}),video:observed('key.video',{}),keySystem:'unsupported'
        });
        const configuration=observed('configuration',{
            audio:observed('audio',audio),video:observed('video',video),keySystemConfiguration:key,
            get type(){audio.channels='8';audio.contentType='invalid';video.width=9000;return 'file';}
        });
        const pending=navigator.mediaCapabilities.decodingInfo(configuration);
        const expected=[
            'configuration.audio','audio.bitrate','audio.channels','audio.contentType','audio.samplerate','audio.spatialRendering',
            'configuration.video','video.bitrate','video.colorGamut','video.contentType','video.framerate',
            'video.hasAlphaChannel','video.hdrMetadataType','video.height','video.scalabilityMode',
            'video.spatialScalability','video.transferFunction','video.width',
            'configuration.keySystemConfiguration','key.audio','key.audio.encryptionScheme','key.audio.robustness',
            'key.distinctiveIdentifier','key.initDataType','key.keySystem','key.persistentState','key.sessionTypes',
            'key.video','key.video.encryptionScheme','key.video.robustness','configuration.type'
        ];
        if(reads.join('|')!==expected.join('|')) throw Error('getter order: '+reads.join('|'));
        pending.then(report=>{
            const copied=report.configuration,key=copied.keySystemConfiguration;
            if(report.supported || copied.audio.channels!=='2' || copied.audio.contentType!==
                'audio/webm;codecs=vorbis' || copied.video.width!==320 || key.distinctiveIdentifier!=='optional' ||
                key.initDataType!=='' || key.persistentState!=='optional' || key.audio.robustness!=='' ||
                key.video.encryptionScheme!==null || Object.keys(copied).join(',')!==
                'audio,video,keySystemConfiguration,type' || Object.keys(key).join(',')!==
                'audio,distinctiveIdentifier,initDataType,keySystem,persistentState,video' ||
                Object.keys(key.audio).join(',')!=='encryptionScheme,robustness')
                throw Error('recursive/default snapshot');
            document.querySelector('output').textContent='passed';
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}

#[test]
fn capability_session_types_require_an_iterable_and_snapshot_each_yielded_value() {
    let (dom, outcome) = execute_html(
        r#"<output>pending</output><script>
        const audio={contentType:'audio/mpeg'};
        const query=sessionTypes=>navigator.mediaCapabilities.decodingInfo({type:'file',audio,
            keySystemConfiguration:{keySystem:'unsupported',sessionTypes}});
        let iteratorReads=0,nextReads=0,index=0;
        const steps=[],values=['temporary',{toString(){steps.push('convert');return 'persistent-license';}}];
        const sequence={get [Symbol.iterator](){iteratorReads++;return function(){
            if(this!==sequence) throw Error('iterator receiver');
            return {get next(){nextReads++;return function(){
                steps.push('next');return index<values.length?{value:values[index++]}:{done:true};
            }}};
        }}};
        const pending=query(sequence);
        values[0]='changed';
        const invalid=[null,'temporary',{0:'temporary',length:1},{[Symbol.iterator]:null},
            {[Symbol.iterator]:1},{[Symbol.iterator](){return 1;}},
            {[Symbol.iterator](){return {next(){return 1;}};}}];
        Promise.all([pending,...invalid.map(value=>query(value).then(()=>false,error=>error.name==='TypeError'))])
        .then(results=>{
            const copy=results[0].configuration.keySystemConfiguration.sessionTypes;
            if(iteratorReads!==1 || nextReads!==1 || steps.join(',')!=='next,next,convert,next' ||
                copy.join(',')!=='temporary,persistent-license' || !results.slice(1).every(Boolean))
                throw Error('sequence conversion');
            document.querySelector('output').textContent='passed';
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}

#[test]
fn capability_validity_runs_after_conversion_and_sequence_errors_do_not_close_iterators() {
    let (dom, outcome) = execute_html(
        r#"<output>pending</output><script>
        const capabilities=navigator.mediaCapabilities;
        const marker=()=>{throw Error('later dictionary conversion');};
        const configurations=[
            {audio:{contentType:'invalid'},get video(){marker();},type:'file'},
            {video:{contentType:'video/mp4;codecs=avc1.42e01e',bitrate:500000,framerate:0,height:240,
                get width(){marker();}},type:'file'}
        ];
        let closed=0,advances=0;
        const sessionTypes={[Symbol.iterator](){return {
            next(){advances++;return {value:Symbol('not a DOMString')};},
            return(){closed++;return {done:true};}
        };}};
        Promise.all([
            ...configurations.map(value=>capabilities.decodingInfo(value)
                .then(()=>false,error=>error.message==='later dictionary conversion')),
            capabilities.decodingInfo({type:'file',audio:{contentType:'audio/mpeg'},
                keySystemConfiguration:{keySystem:'unsupported',sessionTypes}})
                .then(()=>false,error=>error.name==='TypeError')
        ]).then(results=>{
            if(!results.every(Boolean) || closed!==0 || advances!==1) throw Error('conversion/error ordering');
            document.querySelector('output').textContent='passed';
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}

#[test]
fn capability_dom_strings_use_intrinsic_conversion_once_and_reject_boxed_symbols() {
    let (dom, outcome) = execute_html(
        r#"<output>pending</output><script>
        const query=contentType=>navigator.mediaCapabilities.decodingInfo({type:'file',audio:{contentType}});
        let reads=0,calls=0,throwReads=0,throwCalls=0;
        const value={get [Symbol.toPrimitive](){reads++;return function(hint){
            calls++;if(hint!=='string') throw Error('string hint');return 'audio/mpeg';
        }}};
        const throwing={get [Symbol.toPrimitive](){throwReads++;return function(){
            throwCalls++;throw Error('conversion marker');
        }}};
        const originalString=globalThis.String;
        globalThis.String=()=>{throw Error('mutable String was consulted');};
        const converted=query(value);
        const failures=[Symbol('primitive'),Object(Symbol('boxed'))]
            .map(value=>query(value).then(()=>false,error=>error.name==='TypeError'));
        const thrown=query(throwing).then(()=>false,error=>error.message==='conversion marker');
        globalThis.String=originalString;
        Promise.all([converted,...failures,thrown]).then(results=>{
            if(!results[0].supported || results[0].configuration.audio.contentType!=='audio/mpeg' ||
                !results.slice(1).every(Boolean) || reads!==1 || calls!==1 || throwReads!==1 || throwCalls!==1)
                throw Error('DOMString conversion');
            document.querySelector('output').textContent='passed';
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}
