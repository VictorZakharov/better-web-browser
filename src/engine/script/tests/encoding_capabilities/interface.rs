use super::super::*;
use super::verify;

#[test]
fn window_capabilities_have_a_branded_nonconstructible_interface_and_same_object() {
    verify(
        r#"
        const cap=navigator.mediaCapabilities;
        if(typeof MediaCapabilities!=='function' || MediaCapabilities.length!==0 ||
            cap!==navigator.mediaCapabilities || !(cap instanceof MediaCapabilities) ||
            Object.getPrototypeOf(cap)!==MediaCapabilities.prototype ||
            Object.prototype.toString.call(cap)!=='[object MediaCapabilities]') throw Error('interface');
        for(const construct of [()=>new MediaCapabilities(),()=>MediaCapabilities()]) {
            try{construct();throw Error('constructible interface');}
            catch(error){if(error.name!=='TypeError') throw error;}
        }
        for(const name of ['decodingInfo','encodingInfo']) {
            const descriptor=Object.getOwnPropertyDescriptor(MediaCapabilities.prototype,name);
            if(!descriptor.enumerable || !descriptor.configurable || !descriptor.writable ||
                descriptor.value.name!==name || descriptor.value.length!==1 || cap.hasOwnProperty(name))
                throw Error('method descriptor');
        }
        const tag=Object.getOwnPropertyDescriptor(MediaCapabilities.prototype,Symbol.toStringTag);
        if(tag.value!=='MediaCapabilities' || tag.writable || tag.enumerable || !tag.configurable)
            throw Error('interface tag');
        const pending=[];
        for(const name of ['decodingInfo','encodingInfo'])
            for(const receiver of [MediaCapabilities.prototype,Object.create(MediaCapabilities.prototype),
                new Proxy(cap,{})]) pending.push(MediaCapabilities.prototype[name].call(receiver,{}));
        Promise.all(pending.map(p=>p.then(()=>false,error=>error.name==='TypeError'))).then(values=>{
            if(!values.every(Boolean)) throw Error('prototype is not a branded instance');
            document.querySelector('output').textContent='passed';
        });
    "#,
    );
}

#[test]
fn independent_encoding_queries_create_independent_promises_reports_and_snapshots() {
    verify(
        r#"
        const cap=navigator.mediaCapabilities;
        const config=Object.freeze({type:'record',audio:Object.freeze({
            contentType:'audio/webm;codecs=opus',channels:'2',samplerate:48000,bitrate:64000})});
        const first=cap.encodingInfo(config), second=cap.encodingInfo(config);
        if(first===second) throw Error('NewObject promise');
        Promise.all([first,second]).then(([one,two])=>{
            if(!one.supported || !two.supported || one===two || one.configuration===two.configuration ||
                one.configuration.audio===two.configuration.audio ||
                one.configuration.audio===config.audio) throw Error('query result sharing');
            one.configuration.audio.contentType='changed'; one.supported=false;
            if(!two.supported || two.configuration.audio.contentType!==config.audio.contentType)
                throw Error('mutable report isolation');
            document.querySelector('output').textContent='passed';
        });
    "#,
    );
}

#[test]
fn capability_queries_still_work_if_author_replaces_public_constructor_and_methods() {
    verify(
        r#"
        const cap=navigator.mediaCapabilities;
        const encoding=cap.encodingInfo.bind(cap), decoding=cap.decodingInfo.bind(cap);
        window.MediaCapabilities=function Fake(){};
        cap.encodingInfo=()=>{throw Error('replaced public method');};
        Promise.all([
            encoding({type:'record',audio:{contentType:'audio/webm;codecs=opus'}}),
            decoding({type:'file',audio:{contentType:'audio/webm;codecs=opus'}})
        ]).then(results=>{
            if(results.some(r=>!r.supported || r.smooth || r.powerEfficient)) throw Error('closed-over branding');
            document.querySelector('output').textContent='passed';
        });
    "#,
    );
}

#[test]
fn insecure_window_encrypted_queries_reject_security_without_disabling_clear_media() {
    let html = r#"<body><output>pending</output><script>
        const cap=navigator.mediaCapabilities;
        const audio={contentType:'audio/webm;codecs=opus'};
        const config={type:'file',audio,keySystemConfiguration:{keySystem:'org.w3.clearkey'}};
        cap.decodingInfo(config).then(()=>{throw Error('insecure key-system query');},error=>{
            if(error.name!=='SecurityError') throw Error('wrong encrypted-context error '+error.name);
            return cap.decodingInfo({type:'file',audio});
        }).then(report=>{
            if(!report.supported || report.keySystemAccess!==null) throw Error('clear media');
            return cap.encodingInfo({type:'record',audio});
        }).then(report=>{
            if(!report.supported) throw Error('clear encoding query does not require permission');
            document.querySelector('output').textContent='passed';
        });
    </script></body>"#;
    let dom = crate::engine::dom::parse_with_scripting(html, true);
    let script = dom.elements_named("script").next().unwrap();
    let outcome = execute(
        dom.document.clone(),
        "http://example.test/",
        &[ScriptInput {
            source_url: "http://example.test/#inline".into(),
            code: script.text_content(),
            node: script,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        }],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}
