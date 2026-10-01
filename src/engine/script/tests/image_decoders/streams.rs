use super::check;

#[test]
fn streamed_image_waits_for_complete_input_and_decodes_all_chunks() {
    check(
        r#"
        const bytes=pngBytes();let controller;
        const stream=new ReadableStream({start(value){controller=value;}});
        const decoder=new ImageDecoder({type:'image/png',data:stream});
        assert(!decoder.complete&&stream.locked,'stream input incomplete and reader owned');
        const pending=decoder.decode();
        controller.enqueue(bytes.subarray(0,5));controller.enqueue(new Uint8Array(0));
        controller.enqueue(bytes.subarray(5));controller.close();
        await decoder.completed;assert(decoder.complete,'stream complete');
        const {image}=await pending;assert(image.codedWidth===2,'decoded stream');image.close();decoder.close();
        assert(!stream.locked,'reader lock released');
    "#,
    );
}

#[test]
fn stream_close_cancels_reader_and_rejects_completed_and_decode() {
    check(
        r#"
        let cancelled=false;
        const stream=new ReadableStream({cancel(){cancelled=true;}});
        const decoder=new ImageDecoder({type:'image/png',data:stream});
        const pending=decoder.decode(),completed=decoder.completed;decoder.close();
        for(const promise of [pending,completed]){
            let error;try{await promise;}catch(e){error=e.name;}
            assert(error==='AbortError','stream close settles promises');
        }
        await Promise.resolve();await Promise.resolve();
        assert(cancelled,'underlying stream cancelled');
    "#,
    );
}

#[test]
fn wrong_stream_chunk_type_rejects_without_entering_native_codec() {
    check(
        r#"
        const stream=new ReadableStream({start(c){c.enqueue('not bytes');c.close();}});
        const decoder=new ImageDecoder({type:'image/png',data:stream});
        let error;try{await decoder.decode();}catch(e){error=e.name;}
        assert(error==='EncodingError','invalid image stream rejected');decoder.close();
    "#,
    );
}

#[test]
fn locked_or_disturbed_streams_reject_synchronously_without_transfer() {
    check(
        r#"
        const stream=new ReadableStream({start(c){c.enqueue(pngBytes());c.close();}});
        const reader=stream.getReader(),buffer=new ArrayBuffer(1);
        let name;try{new ImageDecoder({type:'image/png',data:stream,transfer:[buffer]});}catch(e){name=e.name;}
        assert(name==='TypeError'&&buffer.byteLength===1,'locked stream does not consume transfer');
        await reader.read();reader.releaseLock();
        assert(!stream.locked,'released disturbed stream');
        try{new ImageDecoder({type:'image/png',data:stream});}catch(e){name=e.name;}
        assert(name==='TypeError'&&!stream.locked,'disturbed stream remains unusable');
    "#,
    );
}

#[test]
fn decoder_stream_uses_intrinsics_not_author_replaced_reader_methods() {
    check(
        r#"
        const bytes=pngBytes(),stream=new ReadableStream({start(c){c.enqueue(bytes);c.close();}});
        const oldRead=ReadableStreamDefaultReader.prototype.read;
        const oldRelease=ReadableStreamDefaultReader.prototype.releaseLock;
        stream.getReader=()=>{throw Error('author getReader must not execute');};
        ReadableStreamDefaultReader.prototype.read=()=>{throw Error('author read must not execute');};
        ReadableStreamDefaultReader.prototype.releaseLock=()=>{throw Error('author release must not execute');};
        const decoder=new ImageDecoder({type:'image/png',data:stream});
        try {
            const {image}=await decoder.decode();assert(image.codedWidth===2,'internal stream read succeeds');
            image.close();assert(!stream.locked,'internal release succeeds');
        } finally {
            decoder.close();ReadableStreamDefaultReader.prototype.read=oldRead;
            ReadableStreamDefaultReader.prototype.releaseLock=oldRelease;
        }
    "#,
    );
}
