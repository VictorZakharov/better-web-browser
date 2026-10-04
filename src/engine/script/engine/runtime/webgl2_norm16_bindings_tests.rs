//! EXT_texture_norm16 tests the public source/view overloads, not enum probes.
use super::webgl2_bindings_tests::{check, document};

#[test]
fn norm16_realm_extension_identity_constants_and_context_isolation() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=new OffscreenCanvas(2,2).getContext('webgl2');
        if (!gl.getSupportedExtensions().includes('EXT_texture_norm16')) throw Error('real provider extension missing');
        const ext=gl.getExtension('ext_TEXTURE_norm16');
        if (!ext || gl.getExtension('EXT_texture_norm16')!==ext) throw Error('extension identity or case');
        if ('EXT_texture_norm16' in globalThis || 'R16_EXT' in gl) throw Error('extension constants leaked');
        const expected={R16_EXT:0x822a,RG16_EXT:0x822c,RGB16_EXT:0x8054,RGBA16_EXT:0x805b,
            R16_SNORM_EXT:0x8f98,RG16_SNORM_EXT:0x8f99,RGB16_SNORM_EXT:0x8f9a,RGBA16_SNORM_EXT:0x8f9b};
        for (const [name,value] of Object.entries(expected)) {
            const descriptor=Object.getOwnPropertyDescriptor(Object.getPrototypeOf(ext),name);
            if (!descriptor || descriptor.value!==value || descriptor.writable || descriptor.configurable || !descriptor.enumerable)
                throw Error('extension constant descriptor '+name);
        }
        if (Object.prototype.toString.call(ext)!=='[object EXT_texture_norm16]') throw Error('extension brand');
        const peer=new OffscreenCanvas(2,2).getContext('webgl2');
        peer.bindTexture(peer.TEXTURE_2D,peer.createTexture());
        peer.texStorage2D(peer.TEXTURE_2D,1,ext.RGBA16_EXT,1,1);
        if (peer.getError()!==peer.INVALID_ENUM) throw Error('peer extension admission inherited');
        const legacy=new OffscreenCanvas(2,2).getContext('webgl');
        if (legacy.getSupportedExtensions().includes('EXT_texture_norm16') || legacy.getExtension('EXT_texture_norm16')!==null)
            throw Error('WebGL2 extension leaked into WebGL1');
    "#,
    );
}

#[test]
fn norm16_realm_unsigned_upload_offsets_and_read_destination_preserve_precision() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=new OffscreenCanvas(2,2).getContext('webgl2'), ext=gl.getExtension('EXT_texture_norm16');
        const texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,texture);
        gl.texImage2D(gl.TEXTURE_2D,0,ext.RGBA16_EXT,1,1,0,gl.RGBA,gl.UNSIGNED_SHORT,
            new Uint16Array([99,1,32769,65534,257,98]),1);
        const fb=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        const pixels=new Uint16Array(6).fill(1234);
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_SHORT,pixels,1);
        if ([...pixels].join()!=='1234,1,32769,65534,257,1234' || gl.getError()!==0) throw Error('16-bit selected view transfer');
        gl.texSubImage2D(gl.TEXTURE_2D,0,0,0,1,1,gl.RGBA,gl.UNSIGNED_SHORT,new Int16Array(4));
        if (gl.getError()!==gl.INVALID_OPERATION) throw Error('signed view accepted for unsigned type');
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_SHORT,pixels,1);
        if ([...pixels].join()!=='1234,1,32769,65534,257,1234') throw Error('wrong view changed storage');
        const canvas=new OffscreenCanvas(1,1), data=new ImageData(new Uint8ClampedArray([255,0,0,255]),1,1);
        for (const source of [canvas,data]) {
            gl.texSubImage2D(gl.TEXTURE_2D,0,0,0,gl.RGBA,gl.UNSIGNED_SHORT,source);
            if (gl.getError()!==gl.INVALID_OPERATION) throw Error('DOM source entered ArrayBufferView-only extension');
        }
    "#,
    );
}

#[test]
fn norm16_realm_pixel_buffers_transfer_unsigned_words_and_preserve_padding() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=new OffscreenCanvas(2,2).getContext('webgl2'), ext=gl.getExtension('EXT_texture_norm16');
        const pbo=gl.createBuffer();gl.bindBuffer(gl.PIXEL_UNPACK_BUFFER,pbo);
        gl.bufferData(gl.PIXEL_UNPACK_BUFFER,new Uint16Array([99,1,32769,65534,257,98]),gl.STATIC_DRAW);
        gl.bindTexture(gl.TEXTURE_2D,gl.createTexture());
        gl.texImage2D(gl.TEXTURE_2D,0,ext.RGBA16_EXT,1,1,0,gl.RGBA,gl.UNSIGNED_SHORT,2);
        if (gl.getError()!==0) throw Error('16-bit PBO upload');
        gl.texSubImage2D(gl.TEXTURE_2D,0,0,0,1,1,gl.RGBA,gl.UNSIGNED_SHORT,3);
        if (gl.getError()!==gl.INVALID_OPERATION) throw Error('unaligned PBO offset accepted');
        gl.bindBuffer(gl.PIXEL_UNPACK_BUFFER,null);
        const fb=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,gl.getParameter(gl.TEXTURE_BINDING_2D),0);
        const pack=gl.createBuffer();gl.bindBuffer(gl.PIXEL_PACK_BUFFER,pack);
        gl.bufferData(gl.PIXEL_PACK_BUFFER,new Uint16Array(6).fill(1234),gl.STREAM_READ);
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_SHORT,2);
        const pixels=new Uint16Array(6);gl.getBufferSubData(gl.PIXEL_PACK_BUFFER,0,pixels);
        const error=gl.getError();
        if ([...pixels].join()!=='1234,1,32769,65534,257,1234' || error!==0) throw Error('16-bit PBO readback precision/padding: '+[...pixels]+' error '+error);
    "#,
    );
}

#[test]
fn norm16_realm_volume_pbo_upload_reads_gpu_written_bytes_and_restores_bindings() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=new OffscreenCanvas(2,2).getContext('webgl2'), ext=gl.getExtension('EXT_texture_norm16');
        const source=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,source);
        gl.texImage2D(gl.TEXTURE_2D,0,ext.RGBA16_EXT,1,1,0,gl.RGBA,gl.UNSIGNED_SHORT,new Uint16Array([1,32769,65534,257]));
        const fb=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,source,0);
        const buffer=gl.createBuffer();gl.bindBuffer(gl.PIXEL_PACK_BUFFER,buffer);
        gl.bufferData(gl.PIXEL_PACK_BUFFER,new Uint16Array(6).fill(1234),gl.STREAM_READ);
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_SHORT,2);
        gl.bindBuffer(gl.PIXEL_PACK_BUFFER,null);
        gl.bindBuffer(gl.PIXEL_UNPACK_BUFFER,buffer);
        for (const target of [gl.TEXTURE_3D,gl.TEXTURE_2D_ARRAY]) {
            const texture=gl.createTexture();gl.bindTexture(target,texture);
            gl.texStorage3D(target,1,ext.RGBA16_EXT,1,1,2);
            gl.texSubImage3D(target,0,0,0,1,1,1,1,gl.RGBA,gl.UNSIGNED_SHORT,2);
            if (gl.getError()!==0 || gl.getParameter(gl.PIXEL_UNPACK_BUFFER_BINDING)!==buffer)
                throw Error('normalized volume GPU buffer transfer or binding');
            gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,null,0);
            gl.framebufferTextureLayer(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,texture,0,1);
            const pixels=new Uint16Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_SHORT,pixels);
            if ([...pixels].join()!=='1,32769,65534,257' || gl.getError()!==0)
                throw Error('GPU-written PBO used stale CPU bytes: '+[...pixels]);
            gl.framebufferTextureLayer(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,texture,0,0);
            gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_SHORT,pixels);
            if ([...pixels].join()!=='0,0,0,0' || gl.getError()!==0) throw Error('volume PBO upload overwrote neighboring layer');
        }
    "#,
    );
}

#[test]
fn norm16_realm_flip_and_premultiply_apply_to_owned_unsigned_views() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=new OffscreenCanvas(2,2).getContext('webgl2'), ext=gl.getExtension('EXT_texture_norm16');
        const texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,texture);
        gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL,true);
        const source=new Uint16Array([65535,0,0,32768,0,65535,0,65535]);
        gl.texImage2D(gl.TEXTURE_2D,0,ext.RGBA16_EXT,1,2,0,gl.RGBA,gl.UNSIGNED_SHORT,source);
        const fb=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        const pixels=new Uint16Array(8);gl.readPixels(0,0,1,2,gl.RGBA,gl.UNSIGNED_SHORT,pixels);
        if ([...pixels].join()!=='0,65535,0,65535,65535,0,0,32768' || gl.getError()!==0) throw Error('16-bit view flip');
        gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL,true);
        gl.texSubImage2D(gl.TEXTURE_2D,0,0,0,1,2,gl.RGBA,gl.UNSIGNED_SHORT,source);
        gl.readPixels(0,0,1,2,gl.RGBA,gl.UNSIGNED_SHORT,pixels);
        if ([...pixels].join()!=='0,65535,0,65535,32768,0,0,32768' || gl.getError()!==0) throw Error('16-bit view premultiplication');
        if ([...source].join()!=='65535,0,0,32768,0,65535,0,65535') throw Error('source view mutated');
    "#,
    );
}
