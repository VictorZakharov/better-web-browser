use super::check;

#[test]
fn persistent_storage_rejects_frames_even_when_nested_without_closing_them() {
    check(
        r#"
        const frame=new VideoFrame(new Uint8Array([1,2,3,255]),{format:'RGBA',codedWidth:1,codedHeight:1,timestamp:7});
        for(const value of [frame,{nested:frame},new Map([['frame',frame]]),new Set([frame])]) {
            let name;try{__serializeClone(value,[],true);}catch(e){name=e.name;}
            assert(name==='DataCloneError'&&frame.codedWidth===1,'storage serialization rejects live frame');
        }
        const clone=structuredClone(frame);assert(clone.timestamp===7,'ordinary clone still permitted');
        frame.close();clone.close();
    "#,
    );
}

#[test]
fn structured_clone_preserves_frame_graph_identity_without_detaching_source() {
    check(
        r#"
        const frame=new VideoFrame(new Uint8Array([1,2,3,255]),{format:'RGBA',codedWidth:1,codedHeight:1,timestamp:-99,duration:22});
        const graph={a:frame,b:frame};graph.self=graph;
        const copy=structuredClone(graph);
        assert(copy.a===copy.b&&copy.self===copy,'frame reference graph');
        assert(copy.a!==frame&&frame.codedWidth===1,'independent frame object');
        assert(copy.a.timestamp===-99&&copy.a.duration===22,'frame timing serialized');
        frame.close();
        const output=new Uint8Array(4);copy.a.copyTo(output).then(()=>assert(output.join(',')==='1,2,3,255','cloned samples'));copy.a.close();
    "#,
    );
}

#[test]
fn transferred_frame_closes_sender_after_serialization_not_before() {
    check(
        r#"
        const frame=new VideoFrame(new Uint8Array([1,2,3,255]),{format:'RGBA',codedWidth:1,codedHeight:1,timestamp:4});
        const received=structuredClone(frame,{transfer:[frame]});
        assert(frame.format===null&&received.format==='RGBA','transfer ownership');
        const output=new Uint8Array(4);received.copyTo(output).then(()=>assert(output.join(',')==='1,2,3,255','transferred samples'));received.close();
    "#,
    );
}

#[test]
fn failed_graph_serialization_does_not_consume_any_frame_or_arraybuffer() {
    check(
        r#"
        const frame=new VideoFrame(new Uint8Array(4),{format:'RGBA',codedWidth:1,codedHeight:1,timestamp:0});
        const buffer=new ArrayBuffer(4);
        for(const graph of [{frame,uncloneable:()=>{}},{frame,uncloneable:Symbol('x')}]){
            let name;try{structuredClone(graph,{transfer:[frame,buffer]});}catch(error){name=error.name;}
            assert(name==='DataCloneError'&&frame.format==='RGBA'&&buffer.byteLength===4,'atomic graph failure');
        }
        frame.close();
    "#,
    );
}

#[test]
fn duplicate_or_closed_frame_transfer_fails_without_mutating_other_transfers() {
    check(
        r#"
        const frame=new VideoFrame(new Uint8Array(4),{format:'RGBA',codedWidth:1,codedHeight:1,timestamp:0});
        const buffer=new ArrayBuffer(4);let name;
        try{structuredClone(frame,{transfer:[buffer,frame,frame]});}catch(error){name=error.name;}
        assert(name==='DataCloneError'&&frame.format==='RGBA'&&buffer.byteLength===4,'duplicate transfer');
        frame.close();
        try{structuredClone(frame,{transfer:[buffer,frame]});}catch(error){name=error.name;}
        assert(name==='DataCloneError'&&buffer.byteLength===4,'closed transfer');
    "#,
    );
}

#[test]
fn frame_transfer_does_not_invoke_author_close_or_color_methods() {
    check(
        r#"
        const frame=new VideoFrame(new Uint8Array([8,9,10,255]),{format:'RGBA',codedWidth:1,codedHeight:1,timestamp:0});
        frame.close=()=>{throw Error('author close');};frame.colorSpace.toJSON=()=>{throw Error('author color');};
        const received=structuredClone(frame,{transfer:[frame]});
        assert(frame.format===null&&received.format==='RGBA','private transfer operation');
        assert(received.colorSpace.matrix==='rgb','private metadata snapshot');received.close();
    "#,
    );
}

#[test]
fn yuv_structured_clone_preserves_planes_crop_and_display_metadata() {
    check(
        r#"
        const frame=new VideoFrame(new Uint8Array([16,30,40,235,128,128]),{format:'I420',codedWidth:2,codedHeight:2,
            timestamp:77,duration:100,rotation:90,flip:true,displayWidth:8,displayHeight:4});
        const copy=structuredClone(frame);frame.close();
        assert(copy.format==='I420'&&copy.rotation===90&&copy.flip,'format/orientation cloned');
        assert(copy.displayWidth===8&&copy.displayHeight===4&&copy.timestamp===77&&copy.duration===100,'metadata cloned');
        const output=new Uint8Array(6);copy.copyTo(output).then(()=>assert(output.join(',')==='16,30,40,235,128,128','YUV planes'));copy.close();
    "#,
    );
}

#[test]
fn private_capabilities_are_removed_from_global_objects_after_bootstrap() {
    check(
        r#"
        for(const name of ['__videoFrameCloneBindings','__imageDecoderStreamBindings','__bindImageDecoderStreams'])
            assert(!(name in globalThis),'private capability leaked '+name);
    "#,
    );
}
