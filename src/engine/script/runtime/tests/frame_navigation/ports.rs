use super::*;

#[test]
fn undeliverable_messages_release_transferred_ports_instead_of_exhausting_quota() {
    let (dom, mut runtime) = start("<body><script></script>");
    for _ in 0..48 {
        evaluate(
            &mut runtime,
            &dom,
            "for(let n=0;n<50;n++){const c=new MessageChannel();postMessage(null,'https://wrong.test',[c.port1,c.port2]);}",
        );
        drain(&mut runtime);
    }
    evaluate(
        &mut runtime,
        &dom,
        "const c=new MessageChannel();c.port1.close();c.port2.close();",
    );
}

#[test]
fn window_messages_clone_blob_and_file_into_receiver_realm_without_author_getters() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        window.result=[];
        const f=document.createElement('iframe');f.sandbox='allow-scripts';
        f.srcdoc=`<script>addEventListener('message',async e=>{
            const [blob,file]=e.data;
            parent.postMessage([blob instanceof Blob,await blob.text(),blob.type,
                file instanceof File,file.name,file.lastModified,await file.text()],'*');
        });<\/script>`;
        document.body.append(f);
        addEventListener('message',e=>result=e.data);
        const blob=new Blob(['hello'],{type:'text/plain'}),file=new File(['file'],'sample.txt',{lastModified:123});
        blob.__bytes.fill(0);
        Object.defineProperty(blob,'type',{get(){throw Error('author getter')}});
        Object.defineProperty(file,'name',{get(){throw Error('author getter')}});
    </script>"#,
    );
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "f.contentWindow.postMessage([blob,file],'*');",
    );
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if(result.join(',')!=='true,hello,text/plain,true,sample.txt,123,file') throw Error(JSON.stringify(result));",
    );
}

#[test]
fn cross_origin_port_transfer_preserves_queue_entanglement_and_receiver_realm() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        window.results=[];
        const f=document.createElement('iframe');f.sandbox='allow-scripts';
        f.srcdoc=`<script>
            addEventListener('message',event=>{
                const port=event.ports[0];
                if (!(port instanceof MessagePort) || event.data.port!==port) throw Error('port identity');
                port.onmessage=e=>port.postMessage([e.data instanceof Map,e.data.get('n'),Object.getPrototypeOf(e.data)===Map.prototype]);
                parent.postMessage('ready','*');
            });
        <\/script>`;
        document.body.append(f);
        const channel=new MessageChannel();
        channel.port1.onmessage=e=>results.push(e.data);
        channel.port1.postMessage(new Map([['n',42]]));
    </script>"#,
    );
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "f.contentWindow.postMessage({port:channel.port2},'*',[channel.port2]); channel.port2.postMessage('detached');",
    );
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if(results.length!==1 || results[0].join(',')!=='true,42,true') throw Error(JSON.stringify(results));",
    );
}

#[test]
fn invalid_port_transfers_do_not_detach_buffers_or_other_ports() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        const channel=new MessageChannel(), buffer=new ArrayBuffer(4);
        const check=fn=>{try {fn();throw Error('accepted')}catch(e){if(e.name!=='DataCloneError') throw e}};
        check(()=>postMessage({},'*',[buffer,channel.port1,channel.port1]));
        check(()=>postMessage({port:channel.port1},'*'));
        check(()=>channel.port1.postMessage({},[channel.port1]));
        if(buffer.byteLength!==4) throw Error('non-atomic transfer');
        if('__endpoint' in channel.port1 || '__nativePortBindings' in globalThis) throw Error('private state exposed');
        window.received=''; channel.port2.onmessage=e=>received=e.data;
        channel.port1.postMessage('still entangled');
    </script>"#,
    );
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if(received!=='still entangled') throw Error(received);",
    );
}

#[test]
fn port_messages_wait_for_start_and_close_disentangles_peers() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        window.events=[]; const channel=new MessageChannel();
        channel.port2.addEventListener('message',e=>events.push(e.data));
        channel.port2.onclose=()=>events.push('closed');
        channel.port1.postMessage('queued');
    </script>"#,
    );
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if(events.length) throw Error(events);channel.port2.start();",
    );
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if(events.join(',')!=='queued') throw Error(events);channel.port1.close();channel.port1.postMessage('dropped');",
    );
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if(events.join(',')!=='queued,closed') throw Error(events);",
    );
}
