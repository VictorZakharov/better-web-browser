//! The external conformance adapter must preserve failures and transport Unicode.
use super::*;

const REPORTER: &str = include_str!("../../../../tests/webgl/khronos-reporter.js");

fn check(setup: &str, assertions: &str) {
    let code = format!(
        "const messages=[];console.log=value=>messages.push(value);{setup}\n{REPORTER}\n{assertions}"
    );
    let (dom, outcome) = execute_html(&format!("<output></output><script>{code}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "pass"
    );
}

#[test]
fn webgl_khronos_reporter_preserves_pass_fail_and_skipped_results() {
    check(
        "",
        r#"
        const h=window.webglTestHarness;
        h.reportResults('case.html',true,'native pixel matches');
        h.reportResults('case.html',false,'native pixel differs');
        h.reportResults('case.html',true,'optional extension unavailable',true);
        h.notifyFinished();
        if(messages.length!==1)throw Error('one completion marker required');
        const marker='__BREEZE_WPT_RESULT__';
        if(!messages[0].startsWith(marker))throw Error('result transport');
        const report=JSON.parse(messages[0].slice(marker.length));
        if(report.overall.status!=='OK' || report.tests.length!==3 ||
            report.tests.map(t=>t.status).join()!=='PASS,FAIL,PRECONDITION_FAILED') throw Error('assertion statuses changed');
        if(report.tests[0].message!==null || report.tests[1].message!=='native pixel differs' ||
            report.tests[2].message!=='optional extension unavailable')throw Error('failure information changed');
        if(report.tests.some(t=>t.stack!==null))throw Error('fabricated stack');
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_khronos_reporter_requires_advertised_capability_instead_of_counting_skips() {
    check(
        "globalThis.__breezeRequiredWebGlExtension='DOES_NOT_EXIST';",
        r#"
        window.webglTestHarness.reportResults('optional.html',true,'upstream does not require support');
        window.webglTestHarness.notifyFinished();
        const report=JSON.parse(messages[0].slice('__BREEZE_WPT_RESULT__'.length));
        if(report.tests.length!==2 || report.tests[0].status!=='PASS' || report.tests[1].status!=='FAIL' ||
            report.tests[1].name!=='Required native capability: DOES_NOT_EXIST')throw Error('missing capability concealed');
        if('__breezeRequiredWebGlExtension' in globalThis)throw Error('reporting configuration leaked');
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_khronos_reporter_capability_preflight_releases_its_native_context() {
    check(
        "globalThis.__breezeRequiredWebGlExtension='ANGLE_instanced_arrays';",
        r#"
        const peers=[];
        for(let i=0;i<8;i++) {
            const gl=new OffscreenCanvas(1,1).getContext('webgl');
            if(!gl)throw Error('reporter retained native context');peers.push(gl);
        }
        window.webglTestHarness.notifyFinished();
        const report=JSON.parse(messages[0].slice('__BREEZE_WPT_RESULT__'.length));
        if(report.tests.length!==1 || report.tests[0].status!=='PASS')throw Error('real native capability missing');
        for(const gl of peers)gl.getExtension('WEBGL_lose_context').loseContext();
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_khronos_reporter_unicode_is_ascii_encoded_without_changing_assertions() {
    check(
        "",
        r#"
        const message='pixel 🎨 café 漢字 '+String.fromCharCode(0xd800)+' tail';
        window.webglTestHarness.reportResults('unicode.html',false,message);
        window.webglTestHarness.notifyFinished();
        if(/[^\x00-\x7f]/.test(messages[0]))throw Error('non-ASCII transport');
        const report=JSON.parse(messages[0].slice('__BREEZE_WPT_RESULT__'.length));
        const expected='pixel 🎨 café 漢字 � tail';
        if(report.tests[0].name!==expected || report.tests[0].message!==expected || report.tests[0].status!=='FAIL')
            throw Error('Unicode assertion changed');
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_khronos_reporter_rejects_duplicate_completion_and_late_assertions() {
    check(
        "",
        r#"
        const h=window.webglTestHarness;h.reportResults('case',true,'first');h.notifyFinished();
        let exceptions=0;
        try{h.notifyFinished()}catch(e){if(e.message.includes('Duplicate'))exceptions++}
        try{h.reportResults('late',true,'too late')}catch(e){if(e.message.includes('after completion'))exceptions++}
        if(exceptions!==2 || messages.length!==1)throw Error('ambiguous completion accepted');
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_khronos_reporter_chunked_transport_preserves_every_result_in_order() {
    let code = format!(
        "const messages=[];console.log=value=>messages.push(value);{REPORTER}\n{}",
        r#"
        const h=window.webglTestHarness;
        for(let i=0;i<96;i++)h.reportResults('chunked.html',i%2===0,'assertion '+i+' 🎨 '+('x'.repeat(128)));
        h.notifyFinished();
        setTimeout(()=>{
            const marker='__BREEZE_WPT_RESULT__',chunkMarker='__BREEZE_WPT_CHUNK__';
            const end=messages.find(m=>m.startsWith(marker));if(!end)throw Error('missing chunk completion');
            const completion=JSON.parse(end.slice(marker.length));
            const chunks=messages.filter(m=>m.startsWith(chunkMarker)).map(m=>JSON.parse(m.slice(chunkMarker.length)));
            if(chunks.length!==completion.chunks || chunks.length<2)throw Error('chunk count');
            let payload='';
            for(let i=0;i<chunks.length;i++){
                if(chunks[i].index!==i || chunks[i].data.length>8192)throw Error('chunk order/budget');
                payload+=chunks[i].data;
            }
            const report=JSON.parse(payload);
            if(report.tests.length!==96)throw Error('results dropped');
            for(let i=0;i<96;i++)if(report.tests[i].status!==(i%2===0?'PASS':'FAIL') ||
                !report.tests[i].name.startsWith('assertion '+i+' 🎨 '))throw Error('chunked assertion changed');
            document.querySelector('canvas').dataset.result='pass';
        },50);
    "#
    );
    // A private simulated clock executes the adapter's normal asynchronous chunks.
    super::webgl_lifecycle::run(&code);
}
