//! DOM images select source subrectangles before tight native upload.
use super::webgl2_bindings_tests::{check, document};

#[test]
fn webgl2_realm_dom_volume_subrectangles_ignore_row_alignment_and_restore_store() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(4,4));
        const pixels=new Uint8ClampedArray(3*6*4);
        for(let y=0;y<6;y++) for(let x=0;x<3;x++) pixels.set([x*50,y*30,100,255],(y*3+x)*4);
        const image=new ImageData(pixels,3,6), texture=gl.createTexture();
        gl.bindTexture(0x8c1a,texture);
        gl.pixelStorei(gl.UNPACK_ALIGNMENT,8); gl.pixelStorei(gl.UNPACK_ROW_LENGTH,999);
        gl.pixelStorei(gl.UNPACK_SKIP_PIXELS,1); gl.pixelStorei(gl.UNPACK_SKIP_ROWS,1);
        gl.pixelStorei(gl.UNPACK_IMAGE_HEIGHT,3);
        gl.texImage3D(0x8c1a,0,0x8058,1,1,2,0,gl.RGBA,gl.UNSIGNED_BYTE,image);
        if (gl.getParameter(gl.UNPACK_ALIGNMENT)!==8 || gl.getParameter(gl.UNPACK_ROW_LENGTH)!==999 ||
            gl.getParameter(gl.UNPACK_SKIP_PIXELS)!==1 || gl.getParameter(gl.UNPACK_SKIP_ROWS)!==1 ||
            gl.getParameter(gl.UNPACK_IMAGE_HEIGHT)!==3) throw Error('image upload changed author pixel-store state');
        const fb=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        const pixel=new Uint8Array(4);
        for (let layer=0;layer<2;layer++) {
            gl.framebufferTextureLayer(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,texture,0,layer);
            gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
            if ([...pixel].join()!==(layer===0?'50,30,100,255':'50,120,100,255')) throw Error('DOM volume selection');
        }
        if (gl.getError()!==0) throw Error('unexpected native error');
    "#,
    );
}

#[test]
fn webgl2_realm_dom_upload_flips_and_premultiplies_without_transforming_typed_views() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(2,2));
        const image=new ImageData(new Uint8ClampedArray([200,100,50,128,0,0,255,255]),1,2);
        const texture=gl.createTexture(); gl.bindTexture(0x8c1a,texture);
        gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL,1); gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL,1);
        gl.texImage3D(0x8c1a,0,0x8058,1,1,2,0,gl.RGBA,gl.UNSIGNED_BYTE,image);
        const fb=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        const pixel=new Uint8Array(4);
        gl.framebufferTextureLayer(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,texture,0,0);
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        if ([...pixel].join()!=='0,0,255,255') throw Error('DOM flip ignored');
        gl.framebufferTextureLayer(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,texture,0,1);
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        if ([...pixel].join()!=='100,50,25,128' || gl.getError()!==0) throw Error('DOM premultiplication ignored');
    "#,
    );
}

#[test]
fn webgl2_realm_dom_legacy_sized_2d_and_red_channel_uploads_are_real() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(2,2));
        const image=new ImageData(new Uint8ClampedArray([123,45,67,255]),1,1);
        const texture=gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D,texture);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.R8,gl.RED,gl.UNSIGNED_BYTE,image);
        const fb=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        const pixel=new Uint8Array(4); gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        if ([...pixel].join()!=='123,0,0,255' || gl.getError()!==0) throw Error('DOM red format conversion');
        gl.texSubImage2D(gl.TEXTURE_2D,0,0,0,1,1,gl.RED,gl.UNSIGNED_BYTE,image);
        if (gl.getError()!==0) throw Error('explicit-size DOM subimage');
    "#,
    );
}
