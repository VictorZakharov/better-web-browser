//! Multi-byte pixels and GPU-written replies keep the public view's exact range.
use super::webgl_owned_copy_tests::{SETUP, both};

const FRAMEBUFFER: &str = r#"
    const texture=gl.createTexture(),fb=gl.createFramebuffer();
    gl.bindTexture(gl.TEXTURE_2D,texture);gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
    const attach=()=>{
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        if(gl.checkFramebufferStatus(gl.FRAMEBUFFER)!==gl.FRAMEBUFFER_COMPLETE)
            throw Error('actual typed target incomplete');
    };
"#;

#[test]
fn webgl_owned_unsigned_integer_pixel_replies_preserve_all_bits_and_element_offsets() {
    both(&format!(
        r#"{SETUP}{FRAMEBUFFER}
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA32UI,1,1,0,gl.RGBA_INTEGER,gl.UNSIGNED_INT,null);
        attach();
        const values=new Uint32Array([0xffffffff,0x80000000,0x12345678,17]);
        gl.clearBufferuiv(gl.COLOR,0,values);
        const output=new Uint32Array(12);output.fill(83);
        gl.readPixels(0,0,1,1,gl.RGBA_INTEGER,gl.UNSIGNED_INT,output.subarray(1,9),2);
        for(let i=0;i<output.length;i++){{
            const expected=i>=3&&i<7?values[i-3]:83;
            if(output[i]!==expected)throw Error('unsigned native reply element '+i+': '+output[i]);
        }}
        values.fill(0);
        const again=new Uint32Array(4);
        gl.readPixels(0,0,1,1,gl.RGBA_INTEGER,gl.UNSIGNED_INT,again);
        expect(again,[0xffffffff,0x80000000,0x12345678,17],'clear must copy its author values');
        if(gl.getError()!==0)throw Error('unsigned native reply error');
    "#
    ));
}

#[test]
fn webgl_owned_signed_integer_pixel_replies_keep_negative_and_boundary_values() {
    both(&format!(
        r#"{SETUP}{FRAMEBUFFER}
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA32I,1,1,0,gl.RGBA_INTEGER,gl.INT,null);
        attach();
        const values=new Int32Array([-2147483648,2147483647,-17,0]);
        gl.clearBufferiv(gl.COLOR,0,values);
        const output=new Int32Array(10);output.fill(83);
        gl.readPixels(0,0,1,1,gl.RGBA_INTEGER,gl.INT,output.subarray(2,8),1);
        for(let i=0;i<output.length;i++){{
            const expected=i>=3&&i<7?values[i-3]:83;
            if(output[i]!==expected)throw Error('signed native reply element '+i+': '+output[i]);
        }}
        if(gl.getError()!==0)throw Error('signed native reply error');
    "#
    ));
}

#[test]
fn webgl_owned_float_pixel_replies_preserve_hdr_and_negative_values_without_byte_clamping() {
    both(&format!(
        r#"{SETUP}{FRAMEBUFFER}
        if(!gl.getExtension('EXT_color_buffer_float'))throw Error('real float color extension unavailable');
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA32F,1,1,0,gl.RGBA,gl.FLOAT,null);
        attach();
        const values=new Float32Array([-2,4,.5,1]);
        gl.clearBufferfv(gl.COLOR,0,values);
        const output=new Float32Array(12);output.fill(83);
        gl.readPixels(0,0,1,1,gl.RGBA,gl.FLOAT,output.subarray(1,9),2);
        for(let i=0;i<output.length;i++){{
            const expected=i>=3&&i<7?values[i-3]:83;
            if(output[i]!==expected)throw Error('HDR native reply element '+i+': '+output[i]);
        }}
        if(gl.getError()!==0)throw Error('float native reply error');
    "#
    ));
}

#[test]
fn webgl_owned_typed_read_rejects_wrong_brands_before_publishing_any_bytes() {
    both(&format!(
        r#"{SETUP}{FRAMEBUFFER}
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA32UI,1,1,0,gl.RGBA_INTEGER,gl.UNSIGNED_INT,null);
        attach();gl.clearBufferuiv(gl.COLOR,0,new Uint32Array([1,2,3,4]));
        for(const output of [new Float32Array(8),new Int32Array(8),new Uint16Array(16)]){{
            output.fill(83);
            gl.readPixels(0,0,1,1,gl.RGBA_INTEGER,gl.UNSIGNED_INT,output,1);
            if(gl.getError()!==gl.INVALID_OPERATION)throw Error('wrong typed destination admitted');
            if(output.some(value=>value!==83))throw Error('failed read published bytes');
        }}
        const output=new Uint32Array(4);
        gl.readPixels(0,0,1,1,gl.RGBA_INTEGER,gl.UNSIGNED_INT,output);
        expect(output,[1,2,3,4],'rejected destination must not poison subsequent read');
        if(gl.getError()!==0)throw Error('typed failure recovery error');
    "#
    ));
}

#[test]
fn webgl_owned_gpu_written_pixel_buffer_reads_native_truth_and_preserves_neighbors() {
    both(&format!(
        r#"{SETUP}
        gl.clearColor(1,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
        const pixelBuffer=gl.createBuffer();gl.bindBuffer(gl.PIXEL_PACK_BUFFER,pixelBuffer);
        const source=new Uint8Array(16);source.fill(83);
        gl.bufferData(gl.PIXEL_PACK_BUFFER,source,gl.DYNAMIC_READ);source.fill(17);
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,4);
        const output=new Uint8Array(24);output.fill(99);
        gl.getBufferSubData(gl.PIXEL_PACK_BUFFER,0,output.subarray(4,20));
        expect(output,[99,99,99,99,83,83,83,83,255,0,0,255,83,83,83,83,83,83,83,83,99,99,99,99],
            'native pixel-pack reply with destination neighbors');
        const client=new Uint8Array(4);client.fill(71);
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,client);
        if(gl.getError()!==gl.INVALID_OPERATION)throw Error('PBO binding accepted client-memory overload');
        expect(client,[71,71,71,71],'rejected mixed overload remains atomic');
        gl.bindBuffer(gl.PIXEL_PACK_BUFFER,null);
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,client);
        expect(client,[255,0,0,255],'ordinary owned client read after PBO unbind');
        if(gl.getError()!==0)throw Error('native PBO reply error');
    "#
    ));
}

#[test]
fn webgl_owned_dataview_upload_and_read_use_intrinsic_byte_ranges() {
    both(&format!(
        r#"{SETUP}
        const storage=new ArrayBuffer(20),view=new DataView(storage,4,8);
        for(let i=0;i<8;i++)view.setUint8(i,i+11);
        // Author-shadowed properties cannot widen the native allocation's range.
        Object.defineProperties(view,{{byteOffset:{{value:0}},byteLength:{{value:20}},buffer:{{value:new ArrayBuffer(1)}}}});
        gl.bufferData(gl.ARRAY_BUFFER,view,gl.DYNAMIC_DRAW);
        expect(read(8),[11,12,13,14,15,16,17,18],'intrinsic DataView upload extent');
        new Uint8Array(storage).fill(0);
        const backing=new Uint8Array(16);backing.fill(83);
        gl.getBufferSubData(gl.ARRAY_BUFFER,0,new DataView(backing.buffer,4,8));
        expect(backing,[83,83,83,83,11,12,13,14,15,16,17,18,83,83,83,83],'DataView native reply extent');
        if(gl.getError()!==0)throw Error('DataView copy error');
    "#
    ));
}

#[test]
fn webgl_owned_resizable_sources_reject_without_changing_previous_native_storage() {
    both(&format!(
        r#"{SETUP}
        const backing=new ArrayBuffer(16,{{maxByteLength:32}}),view=new Uint8Array(backing,4,8);
        view.set([1,2,3,4,5,6,7,8]);
        gl.bufferData(gl.ARRAY_BUFFER,new Uint8Array(view),gl.DYNAMIC_DRAW);
        backing.resize(4);
        expect(read(8),[1,2,3,4,5,6,7,8],'fixed copy survives original resize');
        let rejected=false;
        try{{gl.bufferData(gl.ARRAY_BUFFER,view,gl.DYNAMIC_DRAW);}}catch(error){{rejected=error instanceof TypeError;}}
        if(!rejected)throw Error('out-of-bounds resizable view did not throw');
        expect(read(8),[1,2,3,4,5,6,7,8],'failed resizable upload preserves previous bytes');
        backing.resize(16);new Uint8Array(backing).fill(99);
        rejected=false;
        try{{gl.bufferData(gl.ARRAY_BUFFER,view,gl.DYNAMIC_DRAW);}}catch(error){{rejected=error instanceof TypeError;}}
        if(!rejected)throw Error('in-bounds resizable view did not throw');
        const transferred=structuredClone(backing,{{transfer:[backing]}});
        if(backing.byteLength!==0||transferred.byteLength!==16)throw Error('author transfer failed');
        expect(read(8),[1,2,3,4,5,6,7,8],'detachment preserves native storage');
        if(gl.getError()!==0)throw Error('resizable upload error');
    "#
    ));
}
