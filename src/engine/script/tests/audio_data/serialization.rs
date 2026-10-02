//! Raw resources are transferable, encoded packets are persistently cloneable.
use super::check;

#[test]
fn storage_rejects_live_samples_but_preserves_encoded_packet_bytes() {
    check(
        r#"
        const audio=new AudioData({format:'s16',sampleRate:8000,numberOfFrames:2,numberOfChannels:1,
            timestamp:0,data:new Int16Array([-1,1])});
        for(const value of [audio,{nested:audio},new Map([['audio',audio]]),new Set([audio])]) {
            let failure;try{__serializeClone(value,[],true);}catch(error){failure=error;}
            assert(failure.name==='DataCloneError'&&audio.numberOfFrames===2,'raw resource storage rejection');
        }
        const chunk=new EncodedAudioChunk({type:'delta',timestamp:-7,duration:19,data:new Uint8Array([1,2,3])});
        const record=__serializeClone({a:chunk,b:chunk},[],true);
        const copy=__deserializeClone(record);assert(copy.a===copy.b,'persistent encoded graph alias');
        assert(copy.a.type==='delta'&&copy.a.timestamp===-7&&copy.a.duration===19,'persistent timing');
        const actual=new Uint8Array(3);copy.a.copyTo(actual);assert(actual.join(',')==='1,2,3','persistent bytes');
        audio.close();
    "#,
    );
}

#[test]
fn maps_sets_cycles_and_repeated_resources_preserve_clone_identity() {
    check(
        r#"
        const audio=new AudioData({format:'u8',sampleRate:8000,numberOfFrames:3,numberOfChannels:1,
            timestamp:-9,data:new Uint8Array([0,128,255])});
        const graph={audio,map:new Map([[audio,audio]]),set:new Set([audio])};graph.self=graph;
        const copy=structuredClone(graph);
        assert(copy.self===copy&&copy.audio!==audio,'independent cyclic graph');
        assert(copy.map.get(copy.audio)===copy.audio&&copy.set.has(copy.audio),'container identity');
        audio.close();const actual=new Uint8Array(3);copy.audio.copyTo(actual,{planeIndex:0});
        assert(actual.join(',')==='0,128,255','retained samples');copy.audio.close();
    "#,
    );
}

#[test]
fn invalid_transfer_lists_do_not_consume_other_resources() {
    check(
        r#"
        const audio=new AudioData({format:'u8',sampleRate:8000,numberOfFrames:1,numberOfChannels:1,
            timestamp:0,data:new Uint8Array([128])});
        const buffer=new ArrayBuffer(4);
        const chunk=new EncodedAudioChunk({type:'key',timestamp:0,data:new Uint8Array([1])});
        for(const transfer of [[buffer,audio,audio],[buffer,chunk],[buffer,audio,{}]]) {
            let failure;try{structuredClone(audio,{transfer});}catch(error){failure=error;}
            assert(failure.name==='DataCloneError'&&audio.numberOfFrames===1&&buffer.byteLength===4,
                'prevalidate complete transfer list');
        }
        audio.close();let failure;
        try{structuredClone(audio,{transfer:[buffer,audio]});}catch(error){failure=error;}
        assert(failure.name==='DataCloneError'&&buffer.byteLength===4,'closed source cannot consume a buffer');
    "#,
    );
}

#[test]
fn transfers_use_private_state_not_author_replacements() {
    check(
        r#"
        const audio=new AudioData({format:'s16',sampleRate:8000,numberOfFrames:2,numberOfChannels:1,
            timestamp:88,data:new Int16Array([42,-42])});
        audio.close=()=>{throw Error('author close');};audio.copyTo=()=>{throw Error('author copy');};
        audio.clone=()=>{throw Error('author clone');};
        Object.defineProperty(audio,'numberOfFrames',{get(){throw Error('author dimension');}});
        const received=structuredClone(audio,{transfer:[audio]});
        assert(received.numberOfFrames===2&&received.timestamp===88,'private serialization snapshot');
        assert(Object.getOwnPropertyDescriptor(AudioData.prototype,'format').get.call(audio)===null,'private detach');
        const actual=new Int16Array(2);received.copyTo(actual,{planeIndex:0});
        assert(actual.join(',')==='42,-42','resource is not author method dependent');received.close();
    "#,
    );
}

#[test]
fn chunk_transfer_is_arraybuffer_transfer_not_packet_transfer() {
    check(
        r#"
        const first=new ArrayBuffer(3),unrelated=new ArrayBuffer(4);new Uint8Array(first).set([1,2,3]);
        const packet=new EncodedAudioChunk({type:'key',timestamp:0,data:first,transfer:[first,unrelated]});
        assert(first.byteLength===0&&unrelated.byteLength===0,'all requested buffers consumed');
        const target=new Uint8Array(3);packet.copyTo(target);assert(target.join(',')==='1,2,3','packet resource retained');
        let failure;try{structuredClone(packet,{transfer:[packet]});}catch(error){failure=error;}
        assert(failure.name==='DataCloneError','encoded chunks are not transferables');
        const copy=structuredClone(packet);const bytes=new Uint8Array(3);copy.copyTo(bytes);
        assert(bytes.join(',')==='1,2,3'&&packet.byteLength===3,'encoded clone is independent');
    "#,
    );
}

#[test]
fn clone_records_reject_invalid_layout_instead_of_creating_unbranded_resources() {
    check(
        r#"
        for(const record of [
            {t:'audio-data',id:1,v:{format:'u8',sampleRate:8000,numberOfFrames:9,numberOfChannels:1,timestamp:0},p:'AA=='},
            {t:'audio-data',id:1,v:{format:'not-pcm',sampleRate:8000,numberOfFrames:1,numberOfChannels:1,timestamp:0},p:'AA=='},
            {t:'audio-chunk',id:1,v:{type:'invalid',timestamp:0,duration:null},p:'AA=='},
            {t:'audio-chunk',id:1,v:{type:'key'},p:'AA=='}
        ]) {
            let failure;try{__deserializeClone(JSON.stringify(record));}catch(error){failure=error;}
            assert(failure.name==='DataCloneError','invalid record never escapes receive validation');
        }
    "#,
    );
}
