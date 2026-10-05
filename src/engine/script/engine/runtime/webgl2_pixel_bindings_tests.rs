//! Pixel offsets distinguish view elements from native buffer byte offsets.
use super::webgl2_bindings_tests::{check, document};

#[test]
fn webgl2_realm_unknown_pixel_types_are_not_wrong_view_errors() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl = new OffscreenCanvas(4,4).getContext('webgl2');
        const expect = (invoke, error) => {
            invoke(); if (gl.getError() !== error || gl.getError() !== 0) throw Error('pixel error ordering');
        };
        const texture = gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D,texture);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,1,1,0,gl.RGBA,gl.UNSIGNED_BYTE,null);
        const volume = gl.createTexture(); gl.bindTexture(gl.TEXTURE_3D,volume);
        gl.texImage3D(gl.TEXTURE_3D,0,gl.RGBA8,1,1,1,0,gl.RGBA,gl.UNSIGNED_BYTE,null);
        for (const type of [0,0x8032,0x12345678,0xffffffff]) {
            expect(() => gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,1,1,0,gl.RGBA,type,new Uint8Array(4)),gl.INVALID_ENUM);
            expect(() => gl.texSubImage2D(gl.TEXTURE_2D,0,0,0,1,1,gl.RGBA,type,new Uint8Array(4)),gl.INVALID_ENUM);
            expect(() => gl.texImage3D(gl.TEXTURE_3D,0,gl.RGBA8,1,1,1,0,gl.RGBA,type,new Uint8Array(4)),gl.INVALID_ENUM);
            expect(() => gl.texSubImage3D(gl.TEXTURE_3D,0,0,0,0,1,1,1,gl.RGBA,type,new Uint8Array(4)),gl.INVALID_ENUM);
            expect(() => gl.readPixels(0,0,1,1,gl.RGBA,type,new Uint8Array(4)),gl.INVALID_ENUM);
        }
        expect(() => gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,1,1,0,gl.RGBA,gl.UNSIGNED_BYTE,new Float32Array(4)),gl.INVALID_OPERATION);
        expect(() => gl.texImage3D(gl.TEXTURE_3D,0,gl.RGBA8,1,1,1,0,gl.RGBA,gl.UNSIGNED_BYTE,new Float32Array(4)),gl.INVALID_OPERATION);
        expect(() => gl.texImage2D(gl.TEXTURE_2D,0,gl.DEPTH32F_STENCIL8,1,1,0,gl.DEPTH_STENCIL,
            gl.FLOAT_32_UNSIGNED_INT_24_8_REV,new Uint32Array(2)),gl.INVALID_OPERATION);
        const max = gl.getParameter(gl.MAX_3D_TEXTURE_SIZE);
        expect(() => gl.texImage3D(gl.TEXTURE_3D,0,gl.RGBA,max,max,2,0,gl.RGBA,gl.UNSIGNED_BYTE,new Uint8Array(4)),gl.INVALID_OPERATION);
    "#,
    );
}

#[test]
fn webgl2_realm_volume_offsets_and_layer_attachments_preserve_neighbor_images() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(4,4));
        const texture=gl.createTexture(); gl.bindTexture(0x8c1a,texture);
        gl.texImage3D(0x8c1a,0,0x8058,1,1,2,0,gl.RGBA,gl.UNSIGNED_BYTE,
            new Uint8Array([99,98,255,0,0,255,0,255,0,255,97]),2);
        const framebuffer=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);
        const pixel=new Uint8Array(4);
        for (let layer=0;layer<2;layer++) {
            gl.framebufferTextureLayer(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,texture,0,layer);
            if (gl.checkFramebufferStatus(gl.FRAMEBUFFER)!==gl.FRAMEBUFFER_COMPLETE) throw Error('layer incomplete');
            gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
            if ([...pixel].join()!==(layer===0?'255,0,0,255':'0,255,0,255')) throw Error('source offset or layer order');
        }
        gl.texSubImage3D(0x8c1a,0,0,0,0,1,1,1,gl.RGBA,gl.UNSIGNED_BYTE,new Uint8Array([99,0,0,255,255]),1);
        gl.framebufferTextureLayer(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,texture,0,0);
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        if ([...pixel].join()!=='0,0,255,255' || gl.getError()!==0) throw Error('subimage offset');
    "#,
    );
}

#[test]
fn webgl2_realm_integer_pixel_readback_keeps_destination_element_offsets() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(4,4));
        const texture=gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D,texture);
        gl.texStorage2D(gl.TEXTURE_2D,1,0x8d70,1,1);
        const fb=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        gl.clearBufferuiv(0x1800,0,new Uint32Array([0xffffffff,0x80000000,3,4]));
        const result=new Uint32Array(7); result.fill(99);
        gl.readPixels(0,0,1,1,0x8d99,gl.UNSIGNED_INT,result,2);
        if ([...result].join()!=='99,99,4294967295,2147483648,3,4,99') throw Error('integer pixel domain or destination offset');
        gl.readPixels(0,0,1,1,0x8d99,gl.UNSIGNED_INT,new Int32Array(4));
        if (gl.getError()!==gl.INVALID_OPERATION) throw Error('wrong view type accepted');
        if (gl.getError()!==0) throw Error('unexpected native error');
    "#,
    );
}

#[test]
fn webgl2_realm_pixel_buffer_offsets_round_trip_real_native_pixels() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(4,4),{preserveDrawingBuffer:true});
        gl.clearBufferfv(0x1800,0,[0,0,1,1]);
        const pack=gl.createBuffer(); gl.bindBuffer(0x88eb,pack);
        gl.bufferData(0x88eb,new Uint8Array([9,9,9,9,9,9,9,9,9,9,9,9]),gl.STREAM_READ||0x88e1);
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,4);
        const bytes=new Uint8Array(12); gl.getBufferSubData(0x88eb,0,bytes);
        if ([...bytes].join()!=='9,9,9,9,0,0,255,255,9,9,9,9') throw Error('pixel pack byte offset');
        gl.bindBuffer(0x88eb,null);
        const unpack=gl.createBuffer(); gl.bindBuffer(0x88ec,unpack);
        gl.bufferData(0x88ec,new Uint8Array([9,9,9,9,255,0,0,255,9]),gl.STATIC_DRAW);
        const texture=gl.createTexture(); gl.bindTexture(0x8c1a,texture);
        gl.texImage3D(0x8c1a,0,0x8058,1,1,1,0,gl.RGBA,gl.UNSIGNED_BYTE,4);
        gl.bindBuffer(0x88ec,null);
        const fb=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        gl.framebufferTextureLayer(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,texture,0,0);
        const pixel=new Uint8Array(4); gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        if ([...pixel].join()!=='255,0,0,255' || gl.getError()!==0) throw Error('pixel unpack byte offset');
    "#,
    );
}
