use super::*;

#[test]
fn file_reader_encoding_uses_label_then_mime_with_bom_precedence() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        window.results=[];
        const read=(bytes,type,label)=>{const r=new FileReader();r.onload=()=>results.push(r.result);r.readAsText(new Blob([new Uint8Array(bytes)],{type}),label)};
        read([0x80],'text/plain;charset=windows-1252','invalid-label');
        read([0xe9],'text/plain;charset=utf-8','iso-8859-1');
        read([0xff,0xfe,0x41,0],'text/plain','utf-8');
        read([0xc3,0xa9],'text/plain','invalid-label');
    </script>"#,
    );
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if(results.join(',')!=='€,é,A,é') throw Error(results);",
    );
}

#[test]
fn file_reader_modes_preserve_private_bytes_and_async_state() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        window.results=[];window.events=[];
        const blob=new Blob(['hello'],{type:'text/plain'});
        blob.arrayBuffer=()=>{throw Error('author method')};
        Object.defineProperty(blob,'type',{get(){throw Error('author getter')}});
        const methods=['readAsText','readAsBinaryString','readAsDataURL','readAsArrayBuffer'];
        for(const method of methods) {
            const r=new FileReader();
            if(r.readyState!==r.EMPTY || r.result!==null || r.error!==null) throw Error('initial state');
            r.onloadstart=e=>events.push(e.type+':'+e.isTrusted);
            r.onload=e=>{
                if(r.readyState!==r.DONE || r.error!==null || !e.lengthComputable || e.total!==5) throw Error('completion');
                results.push(method+':'+(r.result instanceof ArrayBuffer ? new TextDecoder().decode(r.result) : r.result));
            };
            r[method](blob);
            if(r.readyState!==r.LOADING || results.length) throw Error('synchronous read');
            try {r.readAsText(blob);throw Error('accepted concurrent read')}
            catch(e){if(e.name!=='InvalidStateError') throw e}
        }
    </script>"#,
    );
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        r#"
        if(results.sort().join('|')!=='readAsArrayBuffer:hello|readAsBinaryString:hello|readAsDataURL:data:text/plain;base64,aGVsbG8=|readAsText:hello')
            throw Error(JSON.stringify(results));
        if(events.length!==4 || events.some(e=>e!=='loadstart:true')) throw Error(events);
        if('__fileReaderSnapshot' in globalThis) throw Error('private hook');
    "#,
    );
}

#[test]
fn file_reader_abort_cancels_old_tasks_and_read_chaining_omits_old_loadend() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        window.events=[];const r=new FileReader(),blob=new Blob(['a']);
        r.onloadstart=()=>events.push('start');
        r.onabort=()=>events.push('abort');
        r.onloadend=()=>events.push('end');
        r.onload=()=>{events.push('load');if(events.filter(e=>e==='load').length===1)r.readAsText(blob)};
        r.readAsText(blob);r.abort();
        if(r.readyState!==r.DONE || r.result!==null || r.error!==null) throw Error('abort state');
        r.readAsText(blob);
    </script>"#,
    );
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if(events.join(',')!=='abort,end,start,load,start,load,end') throw Error(events);",
    );
}

#[test]
fn large_file_read_yields_between_bounded_ranges_before_result_is_available() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        window.events=[];
        const bytes = new Uint8Array(3 * 65536 + 7);
        bytes[0] = 11; bytes[bytes.length - 1] = 29;
        window.reader = new FileReader();
        reader.onloadstart = event => events.push('start:' + event.loaded);
        reader.onprogress = event => events.push('progress:' + event.loaded);
        reader.onload = event => events.push('load:' + event.loaded);
        reader.onloadend = event => events.push('end:' + event.loaded);
        reader.readAsArrayBuffer(new Blob([bytes]));
    </script>"#,
    );
    for _ in 0..2 {
        let outcome = runtime.advance_time(Duration::ZERO, 1);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    }
    evaluate(
        &mut runtime,
        &dom,
        "if(reader.readyState!==reader.LOADING || reader.result!==null || events.join(',')!=='start:0') throw Error(events);",
    );
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "const b=new Uint8Array(reader.result); if(reader.readyState!==reader.DONE || b.length!==196615 || b[0]!==11 || b[b.length-1]!==29 || events.at(-3)!=='progress:196615' || events.at(-2)!=='load:196615' || events.at(-1)!=='end:196615') throw Error(events);",
    );
}

#[test]
fn abort_after_partial_file_read_cancels_remaining_ranges_and_reports_loaded_bytes() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        window.events=[];
        window.reader=new FileReader();
        reader.onloadstart=()=>events.push('start');
        reader.onabort=event=>events.push('abort:'+event.loaded+'/'+event.total);
        reader.onload=()=>events.push('load');
        reader.onloadend=()=>events.push('end');
        reader.readAsArrayBuffer(new Blob([new Uint8Array(3*65536+7)]));
    </script>"#,
    );
    for _ in 0..2 {
        let outcome = runtime.advance_time(Duration::ZERO, 1);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    }
    evaluate(&mut runtime, &dom, "reader.abort();");
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if(events.join(',')!=='start,abort:65536/196615,end' || reader.readyState!==reader.DONE || reader.result!==null) throw Error(events);",
    );
}
