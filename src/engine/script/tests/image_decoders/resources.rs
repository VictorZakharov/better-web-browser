use super::check;

#[test]
fn too_many_pending_requests_fail_without_aborting_existing_requests() {
    check(
        r#"
        const decoder=new ImageDecoder({type:'image/png',data:pngBytes()});await decoder.tracks.ready;
        const pending=Array.from({length:32},()=>decoder.decode());
        let name;try{await decoder.decode();}catch(e){name=e.name;}
        assert(name==='NotSupportedError','bounded pending outputs reject excess request');
        const output=await Promise.all(pending);
        assert(output.length===32&&output.every(item=>item.image.codedWidth===2),'accepted requests finish');
        for(const result of output)result.image.close();
        (await decoder.decode()).image.close();decoder.close();
    "#,
    );
}

#[test]
fn invalid_transfer_dictionary_does_not_lock_stream_or_detach_buffers() {
    check(
        r#"
        const stream=new ReadableStream(),buffer=new ArrayBuffer(4);
        let name;try{new ImageDecoder({type:'image/png',data:stream,desiredWidth:1,transfer:[buffer]});}
        catch(e){name=e.name;}
        assert(name==='TypeError'&&!stream.locked&&buffer.byteLength===4,'dictionary failure is atomic');
        const bytes=pngBytes();
        try{new ImageDecoder({type:'image/png',data:bytes,transfer:[bytes.buffer,{}]});}catch(e){name=e.name;}
        assert(name==='TypeError'&&bytes.byteLength>0,'non-transferable fails before mutation');
    "#,
    );
}

#[test]
fn closing_large_decode_before_chunks_finish_cancels_copy_timers() {
    check(
        r#"
        const decoder=new ImageDecoder({type:'image/png',data:pngBytes(),desiredWidth:512,desiredHeight:512});
        await decoder.tracks.ready;
        const pending=decoder.decode();decoder.close();
        let name;try{await pending;}catch(e){name=e.name;}
        assert(name==='AbortError','accepted output cancelled by close');
        assert(decoder.complete&&decoder.tracks.length===0,'complete input remains complete after close');
    "#,
    );
}
