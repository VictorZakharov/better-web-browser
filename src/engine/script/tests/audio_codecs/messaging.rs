use super::check;

#[test]
fn native_message_channel_preserves_audio_samples_and_graph_identity() {
    check(
        r#"
        const channel=new MessageChannel();
        const audio=tone(12,2,48000,-77), chunk=new EncodedAudioChunk({type:'key',timestamp:-77,data:new Uint8Array([1,2,3])});
        const expected=new Float32Array(24);audio.copyTo(expected,{planeIndex:0});
        const result=new Promise(resolve=>channel.port2.onmessage=resolve);
        channel.port1.postMessage({audio,alias:audio,chunk});audio.close();
        const event=await result, value=event.data;
        assert(value.audio instanceof AudioData&&value.audio===value.alias,'receiver brand and alias');
        const actual=new Float32Array(24);value.audio.copyTo(actual,{planeIndex:0});
        assert(actual.join(',')===expected.join(',')&&value.audio.timestamp===-77,'native sample snapshot');
        assert(value.chunk instanceof EncodedAudioChunk&&value.chunk.duration===null,'chunk brand and null duration');
        const bytes=new Uint8Array(3);value.chunk.copyTo(bytes);assert(bytes.join(',')==='1,2,3','packet bytes');
        value.audio.close();channel.port1.close();channel.port2.close();
    "#,
    );
}

#[test]
fn native_audio_transfer_closes_sender_only_after_whole_graph_succeeds() {
    check(
        r#"
        const channel=new MessageChannel(), audio=tone(8), buffer=new ArrayBuffer(4);
        let name;try{channel.port1.postMessage({audio,bad:()=>{}},[audio,buffer]);}catch(e){name=e.name;}
        assert(name==='DataCloneError'&&audio.numberOfFrames===8&&buffer.byteLength===4,'failed graph rollback');
        const received=new Promise(resolve=>channel.port2.onmessage=resolve);
        channel.port1.postMessage({audio,buffer},[audio,buffer]);
        assert(audio.numberOfFrames===0&&buffer.byteLength===0,'committed resources detached');
        const event=await received;assert(event.data.audio.numberOfFrames===8&&event.data.buffer.byteLength===4,'received resources');
        event.data.audio.close();channel.port1.close();channel.port2.close();
    "#,
    );
}

#[test]
fn native_audio_transfer_rejects_duplicates_closed_data_and_chunks() {
    check(
        r#"
        const channel=new MessageChannel(), audio=tone(2), chunk=new EncodedAudioChunk({type:'key',timestamp:0,data:new Uint8Array(1)});
        const fails=(value,transfer)=>{let name;try{channel.port1.postMessage(value,transfer);}catch(e){name=e.name;}assert(name==='DataCloneError','invalid transfer');};
        fails(audio,[audio,audio]);assert(audio.numberOfFrames===2,'duplicate leaves live');
        fails(chunk,[chunk]);audio.close();fails(audio,[]);fails({},[audio]);
        channel.port1.close();channel.port2.close();
    "#,
    );
}

#[test]
fn native_message_clone_ignores_author_sample_getters_and_close_override() {
    check(
        r#"
        const channel=new MessageChannel(), audio=tone(3), originalClose=AudioData.prototype.close;
        Object.defineProperty(audio,'numberOfFrames',{get(){throw Error('author getter');}});
        audio.close=()=>{throw Error('author close');};
        const received=new Promise(resolve=>channel.port2.onmessage=resolve);
        channel.port1.postMessage(audio,[audio]);
        const value=(await received).data;assert(value.numberOfFrames===3,'private snapshot');
        originalClose.call(value);channel.port1.close();channel.port2.close();
    "#,
    );
}

#[test]
fn window_post_message_transfers_audio_to_receiving_realm() {
    check(
        r#"
        const audio=tone(5), received=new Promise(resolve=>addEventListener('message',resolve,{once:true}));
        postMessage({audio},location.origin,[audio]);assert(audio.format===null,'sender closes');
        const event=await received;assert(event.data.audio instanceof AudioData&&event.data.audio.numberOfFrames===5,'receiver samples');
        event.data.audio.close();
        assert(globalThis.__audioMessageCloneBindings===undefined,'private hooks hidden');
    "#,
    );
}

#[test]
fn audio_closed_by_graph_getter_rolls_back_other_native_transfers() {
    check(
        r#"
        const audio=tone(4), buffer=new ArrayBuffer(4), channel=new MessageChannel();
        const graph={audio,get later(){audio.close();return 1;}};
        let name;try{channel.port1.postMessage(graph,[buffer,audio]);}catch(e){name=e.name;}
        assert(name==='DataCloneError'&&buffer.byteLength===4,'late validation before buffer detach');
        channel.port1.close();channel.port2.close();
    "#,
    );
}

#[test]
fn child_frame_receives_its_own_audio_constructor_and_can_transfer_back() {
    check(
        r#"
        const frame=document.createElement('iframe');document.body.append(frame);
        const ready=new Promise(resolve=>frame.addEventListener('load',resolve,{once:true}));
        frame.srcdoc='<body>audio receiver</body>';await ready;
        const child=frame.contentWindow, audio=tone(9,2,48000,456);
        const expected=new Float32Array(18);audio.copyTo(expected,{planeIndex:0});
        const reply=new Promise(resolve=>addEventListener('message',resolve,{once:true}));
        child.addEventListener('message',event=>{
            const value=event.data.audio;
            assert(value instanceof child.AudioData&&!(value instanceof AudioData),'receiving realm prototype');
            const bytes=new Float32Array(18);value.copyTo(bytes,{planeIndex:0});
            assert(bytes.join(',')===expected.join(',')&&value.timestamp===456,'cross-frame PCM');
            child.parent.postMessage({audio:value},location.origin,[value]);
            assert(value.format===null,'child sender detached');
        },{once:true});
        child.postMessage({audio},location.origin,[audio]);assert(audio.format===null,'parent sender detached');
        const value=(await reply).data.audio;
        assert(value instanceof AudioData&&value.numberOfFrames===9,'parent receiving realm');value.close();
    "#,
    );
}

#[test]
fn native_audio_transfer_can_share_a_transaction_with_message_ports() {
    check(
        r#"
        const channel=new MessageChannel(), nested=new MessageChannel(), audio=tone(3);
        const reply=new Promise(resolve=>nested.port1.onmessage=resolve);
        channel.port2.onmessage=event=>{
            const value=event.data.audio;assert(value.numberOfFrames===3,'audio arrives with port');
            assert(event.ports[0]===event.data.port,'port graph and event alias');
            event.data.port.postMessage('pong');event.data.port.close();value.close();
        };
        channel.port1.postMessage({audio,port:nested.port2},[audio,nested.port2]);
        assert(audio.format===null,'audio committed');assert((await reply).data==='pong','port remains entangled');
        nested.port1.close();channel.port1.close();channel.port2.close();
    "#,
    );
}

#[test]
fn native_audio_quota_validation_precedes_all_transfer_detachment() {
    check(
        r#"
        const channel=new MessageChannel(), buffer=new ArrayBuffer(2), audio=new AudioData({
            format:'u8',sampleRate:8000,numberOfChannels:1,numberOfFrames:16*1024*1024,
            timestamp:0,data:new Uint8Array(16*1024*1024)});
        let name;try{channel.port1.postMessage({audio,buffer},[audio,buffer]);}catch(e){name=e.name;}
        assert(name==='QuotaExceededError','native graph budget includes snapshot envelope');
        assert(audio.numberOfFrames===16*1024*1024&&buffer.byteLength===2,'quota does not detach');
        audio.close();channel.port1.close();channel.port2.close();
    "#,
    );
}

#[test]
fn native_audio_clone_retains_all_sample_formats_without_float_quantization() {
    check(
        r#"
        const channel=new MessageChannel(), formats=['u8','s16','s32','f32',
            'u8-planar','s16-planar','s32-planar','f32-planar'];
        for(const format of formats) {
            const kind=format.split('-')[0],stride={u8:1,s16:2,s32:4,f32:4}[kind];
            const bytes=new Uint8Array(7*3*stride);
            // Include arbitrary float bit patterns, not just normalized samples.
            // Cloning is byte preservation even when a sample is NaN or infinity.
            for(let index=0;index<bytes.length;index++)bytes[index]=(index*73+19)&255;
            const audio=new AudioData({format,sampleRate:11025,numberOfFrames:7,
                numberOfChannels:3,timestamp:-90,data:bytes});
            const received=new Promise(resolve=>channel.port2.onmessage=resolve);
            channel.port1.postMessage(audio);audio.close();bytes.fill(0);
            const value=(await received).data;
            assert(value.format===format&&value.sampleRate===11025&&value.numberOfChannels===3,'metadata '+format);
            const planeCount=format.endsWith('-planar')?3:1;
            for(let plane=0;plane<planeCount;plane++){
                const actual=new Uint8Array(value.allocationSize({planeIndex:plane}));
                value.copyTo(actual,{planeIndex:plane});
                const offset=plane*7*stride;
                for(let index=0;index<actual.length;index++)
                    assert(actual[index]===((offset+index)*73+19&255),'native sample bit '+format+' '+index);
            }
            value.close();
        }
        channel.port1.close();channel.port2.close();
    "#,
    );
}

#[test]
fn audio_transfer_list_detaches_resources_not_reachable_from_the_message() {
    check(
        r#"
        const channel=new MessageChannel(), audio=tone(1);
        const received=new Promise(resolve=>channel.port2.onmessage=resolve);
        channel.port1.postMessage('unrelated graph',[audio]);
        assert(audio.format===null&&audio.numberOfFrames===0,'unused transfer still commits');
        assert((await received).data==='unrelated graph','graph stays unchanged');
        channel.port1.close();channel.port2.close();
    "#,
    );
}

#[test]
fn a_detached_port_rolls_back_an_audio_transfer_in_the_same_list() {
    check(
        r#"
        const channel=new MessageChannel(), nested=new MessageChannel(), audio=tone(1);
        channel.port1.postMessage('port',[nested.port2]);
        let name;try{channel.port1.postMessage(audio,[audio,nested.port2]);}catch(e){name=e.name;}
        assert(name==='DataCloneError'&&audio.numberOfFrames===1,'invalid port leaves audio live');
        const delivered=new Promise(resolve=>channel.port2.onmessage=resolve);
        (await delivered).ports[0].close();audio.close();nested.port1.close();
        channel.port1.close();channel.port2.close();
    "#,
    );
}

#[test]
fn encoded_chunk_native_cloning_does_not_invoke_author_metadata_properties() {
    check(
        r#"
        const chunk=new EncodedAudioChunk({type:'delta',timestamp:12,duration:4,data:new Uint8Array([7,8])});
        for(const name of ['type','timestamp','duration','byteLength'])
            Object.defineProperty(chunk,name,{get(){throw Error('author metadata getter');}});
        chunk.copyTo=()=>{throw Error('author copyTo');};
        const channel=new MessageChannel(), delivered=new Promise(resolve=>channel.port2.onmessage=resolve);
        channel.port1.postMessage(chunk);const copy=(await delivered).data;
        assert(copy.type==='delta'&&copy.timestamp===12&&copy.duration===4&&copy.byteLength===2,'private packet metadata');
        const bytes=new Uint8Array(2);copy.copyTo(bytes);assert(bytes.join(',')==='7,8','private packet bytes');
        channel.port1.close();channel.port2.close();
    "#,
    );
}
