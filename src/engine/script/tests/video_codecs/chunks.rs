//! Encoded video is immutable and serializable, not an author-controlled byte accessor.
use super::*;

#[test]
fn encoded_video_snapshots_only_the_source_view_and_copies_to_bounded_destinations() {
    check(
        r#"
        const source=new Uint8Array([99,1,2,3,88]);
        const chunk=new EncodedVideoChunk({type:'delta',timestamp:-17,duration:21,
            data:new DataView(source.buffer,1,3)});
        source.fill(0);
        const target=new Uint8Array(7);target.fill(9);chunk.copyTo(new DataView(target.buffer,2,3));
        assert(target.join(',')==='9,9,1,2,3,9,9','view extent and immutable snapshot');
        assert(chunk.type==='delta'&&chunk.timestamp===-17&&chunk.duration===21&&chunk.byteLength===3,'chunk metadata');
        assert(Object.prototype.toString.call(chunk)==='[object EncodedVideoChunk]','Web IDL tag');
        let failure;try{chunk.copyTo(new Uint8Array(2));}catch(error){failure=error;}
        assert(failure?.name==='TypeError','small destination rejected');
        const untouched=new Uint8Array([7,7]);
        try{chunk.copyTo(untouched);}catch(_){}
        assert(untouched.join(',')==='7,7','failed copy is atomic');
    "#,
    );
}

#[test]
fn video_chunk_clone_preserves_aliases_and_never_calls_author_getters() {
    check(
        r#"
        const source=new EncodedVideoChunk({type:'key',timestamp:7,data:new Uint8Array([1,2,3])});
        for(const name of ['type','timestamp','duration','byteLength'])Object.defineProperty(source,name,
            {get(){throw Error('author getter read');}});
        source.copyTo=()=>{throw Error('author copy used');};
        const clone=structuredClone({a:source,b:source});
        assert(clone.a instanceof EncodedVideoChunk&&clone.a===clone.b,'receiving constructor and aliases');
        const bytes=new Uint8Array(3);clone.a.copyTo(bytes);
        assert(bytes.join(',')==='1,2,3'&&clone.a.timestamp===7&&clone.a.duration===null,'private cloned bytes');
        const buffer=new ArrayBuffer(8);let error;
        try{structuredClone(source,{transfer:[buffer,source]});}catch(failure){error=failure;}
        assert(error?.name==='DataCloneError'&&!buffer.detached,'chunks are not transferable; graph failure is atomic');
    "#,
    );
}

#[test]
fn message_port_video_chunk_clone_restores_video_not_audio_brand() {
    check(
        r#"
        const port=new MessageChannel(),received=new Promise(resolve=>port.port2.onmessage=resolve);
        const chunk=new EncodedVideoChunk({type:'key',timestamp:-9,data:new Uint8Array([8,7,6])});
        port.port1.postMessage({chunk,alias:chunk});
        const value=(await received).data;
        assert(value.chunk instanceof EncodedVideoChunk&&!(value.chunk instanceof EncodedAudioChunk),'native video brand');
        assert(value.chunk===value.alias&&value.chunk.timestamp===-9,'native graph identity');
        const bytes=new Uint8Array(3);value.chunk.copyTo(bytes);assert(bytes.join(',')==='8,7,6','native message bytes');
        port.port1.close();port.port2.close();
    "#,
    );
}

#[test]
fn video_chunk_constructor_transfer_validates_every_buffer_before_detaching_any() {
    check(
        r#"
        const buffer=new Uint8Array([1,2,3]).buffer;
        let error;try{new EncodedVideoChunk({type:'key',timestamp:0,data:buffer,transfer:[buffer,buffer]});}
        catch(failure){error=failure;}
        assert(error?.name==='DataCloneError'&&!buffer.detached,'duplicate constructor transfer remains atomic');
        const chunk=new EncodedVideoChunk({type:'key',timestamp:0,data:buffer,transfer:[buffer]});
        assert(buffer.detached&&chunk.byteLength===3,'ownership accepted after full validation');
        const bytes=new Uint8Array(3);chunk.copyTo(bytes);assert(bytes.join(',')==='1,2,3','retained bytes');
    "#,
    );
}
