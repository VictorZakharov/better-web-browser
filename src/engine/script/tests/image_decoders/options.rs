use super::check;

#[test]
fn invalid_init_and_required_members_throw_synchronously() {
    check(
        r#"
        for(const init of [undefined,42,{}, {type:'text/plain',data:pngBytes()},
            {type:'image/png',data:new Uint8Array(0)}, {type:'image/png',data:new Blob([])},
            {type:'image/png',data:pngBytes(),desiredWidth:1},
            {type:'image/png',data:pngBytes(),desiredWidth:-1,desiredHeight:1},
            {type:'image/png',data:pngBytes(),colorSpaceConversion:'invalid'}]) {
            let thrown=false;try{new ImageDecoder(init);}catch(e){thrown=e instanceof TypeError;}
            assert(thrown,'invalid ImageDecoderInit');
        }
    "#,
    );
}

#[test]
fn transferred_input_is_detached_only_after_snapshot_and_validation() {
    check(
        r#"
        const data=pngBytes(),buffer=data.buffer;
        const decoder=new ImageDecoder({type:'image/png',data,transfer:[buffer]});
        assert(buffer.byteLength===0,'synchronous input transfer');
        (await decoder.decode()).image.close();decoder.close();
        const duplicate=pngBytes();let error;
        try{new ImageDecoder({type:'image/png',data:duplicate,transfer:[duplicate.buffer,duplicate.buffer]});}
        catch(e){error=e.name;}
        assert(error==='DataCloneError'&&duplicate.byteLength>0,'duplicate transfer atomicity');
    "#,
    );
}

#[test]
fn unsupported_image_type_constructs_but_promises_reject_with_not_supported() {
    check(
        r#"
        const decoder=new ImageDecoder({type:'image/not-a-codec',data:pngBytes()});
        assert(decoder.type==='image/not-a-codec','reflect type');
        let error;try{await decoder.decode();}catch(e){error=e.name;}
        assert(error==='NotSupportedError','asynchronous unsupported codec');decoder.close();
    "#,
    );
}

#[test]
fn desired_size_changes_actual_frame_pixels_not_only_metadata() {
    check(
        r#"
        const decoder=new ImageDecoder({type:'image/png',data:pngBytes(),desiredWidth:4,desiredHeight:2});
        const {image}=await decoder.decode();
        assert(image.codedWidth===4&&image.codedHeight===2&&image.allocationSize()===32,'resized bitmap');
        const bytes=new Uint8Array(32);await image.copyTo(bytes);
        assert(bytes[0]===10&&bytes[3]===255,'resized first pixel');image.close();decoder.close();
    "#,
    );
}
