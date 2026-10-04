//! Native texture transfers exercise the core overload's selected owned range.
use super::webgl2_bindings_tests::{check, document};

#[test]
fn webgl2_realm_sized_2d_uploads_and_subimage_offsets_preserve_real_pixels() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(2,2));
        const texture=gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D,texture);
        gl.texImage2D(gl.TEXTURE_2D,0,0x8058,2,1,0,gl.RGBA,gl.UNSIGNED_BYTE,
            new Uint8Array([99,255,0,0,255,0,255,0,255,98]),1);
        gl.texSubImage2D(gl.TEXTURE_2D,0,1,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,
            new Uint8Array([99,0,0,255,255,98]),1);
        const fb=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        const pixels=new Uint8Array(8); gl.readPixels(0,0,2,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
        if ([...pixels].join()!=='255,0,0,255,0,0,255,255' || gl.getError()!==0) throw Error('2D selected transfer range');
    "#,
    );
}

#[test]
fn webgl2_realm_compressed_upload_element_offsets_and_explicit_lengths_match() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(4,4));
        const extension=gl.getExtension('WEBGL_compressed_texture_s3tc');
        if (!extension) throw Error('required desktop compressed family');
        const texture=gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D,texture);
        // DXT1 stores RGB565 endpoints plus sixteen 2-bit selectors.
        const source=new Uint16Array([0x1234,0xf800,0,0,0,0x5678]);
        gl.compressedTexImage2D(gl.TEXTURE_2D,0,extension.COMPRESSED_RGBA_S3TC_DXT1_EXT,4,4,0,source,1,4);
        if (gl.getError()!==0) throw Error('compressed view element offsets');
        gl.compressedTexSubImage2D(gl.TEXTURE_2D,0,0,0,4,4,extension.COMPRESSED_RGBA_S3TC_DXT1_EXT,source,1,4);
        if (gl.getError()!==0) throw Error('compressed subimage selected range');
        gl.compressedTexImage2D(gl.TEXTURE_2D,0,extension.COMPRESSED_RGBA_S3TC_DXT1_EXT,4,4,0,source,1,3);
        if (gl.getError()!==gl.INVALID_VALUE) throw Error('short block range accepted');
        const pbo=gl.createBuffer(); gl.bindBuffer(0x88ec,pbo);
        gl.bufferData(0x88ec,source,gl.STATIC_DRAW);
        gl.compressedTexImage2D(gl.TEXTURE_2D,0,extension.COMPRESSED_RGBA_S3TC_DXT1_EXT,4,4,0,8,2);
        if (gl.getError()!==0) throw Error('compressed PBO image-size/byte-offset overload');
        gl.bindBuffer(0x88ec,null);
        gl.bindTexture(0x8c1a,texture);
        if (gl.getError()!==gl.INVALID_OPERATION) throw Error('texture target changed');
        const array=gl.createTexture(); gl.bindTexture(0x8c1a,array);
        gl.compressedTexImage3D(0x8c1a,0,extension.COMPRESSED_RGBA_S3TC_DXT1_EXT,4,4,1,0,source,1,4);
        if (gl.getError()!==0) throw Error('compressed array selected range');
    "#,
    );
}

#[test]
fn webgl2_realm_forbidden_3d_flip_and_premultiplication_leave_storage_unchanged() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(2,2));
        const texture=gl.createTexture(); gl.bindTexture(0x8c1a,texture);
        gl.texStorage3D(0x8c1a,1,0x8058,1,1,1);
        const bytes=new Uint8Array([255,0,0,255]);
        for (const pname of [gl.UNPACK_FLIP_Y_WEBGL,gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL]) {
            gl.pixelStorei(pname,1);
            gl.texSubImage3D(0x8c1a,0,0,0,0,1,1,1,gl.RGBA,gl.UNSIGNED_BYTE,bytes);
            if (gl.getError()!==gl.INVALID_OPERATION) throw Error('forbidden volume transform accepted');
            gl.pixelStorei(pname,0);
        }
        const fb=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        gl.framebufferTextureLayer(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,texture,0,0);
        const pixel=new Uint8Array(4); gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        if ([...pixel].join()!=='0,0,0,0' || gl.getError()!==0) throw Error('rejected transform mutated texture');
    "#,
    );
}
