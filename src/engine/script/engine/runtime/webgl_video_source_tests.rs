//! Public video texture overloads read current trusted decoder frames, not DOM properties.
use super::webgl2_bindings_tests::{check, document};
use crate::engine::{DecodedImage, script::HostState};
use std::{cell::RefCell, rc::Rc};

fn frame(host: &Rc<RefCell<HostState>>, clean: bool, pixels: &[u8]) {
    let mut host = host.borrow_mut();
    let node = host
        .nodes
        .values()
        .find(|node| node.tag_name() == Some("video"))
        .unwrap()
        .id();
    host.media_images
        .replace(
            node,
            DecodedImage {
                width: 2,
                height: 2,
                bgra: pixels.to_vec().into(),
            },
            clean,
        )
        .unwrap();
}

const SOURCE: &[u8] = &[
    0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255, 255, 255, 255, 255,
];

#[test]
fn video_textures_upload_current_pixels_and_preserve_the_source() {
    let (mut context, host) = document();
    check(
        &mut context,
        r#"
        const video=document.createElement('video'); document.body.append(video);
        const gl=new OffscreenCanvas(2,2).getContext('webgl2');
        const texture=gl.createTexture(),fb=gl.createFramebuffer();
        gl.bindTexture(gl.TEXTURE_2D,texture);gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
    "#,
    );
    frame(&host, true, SOURCE);
    check(
        &mut context,
        r#"
        // Writable accessors and a newly set CORS attribute are not native authority.
        Object.defineProperty(video,'videoWidth',{value:9999}); video.crossOrigin='anonymous';
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,gl.RGBA,gl.UNSIGNED_BYTE,video);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        const pixels=new Uint8Array(16);gl.readPixels(0,0,2,2,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
        if (pixels.join()!=='255,0,0,255,0,255,0,255,0,0,255,255,255,255,255,255'||gl.getError())
            throw Error('wrong decoded video texture '+pixels);
        gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL,true);
        gl.texSubImage2D(gl.TEXTURE_2D,0,0,0,gl.RGBA,gl.UNSIGNED_BYTE,video);
        gl.readPixels(0,0,2,2,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
        if (pixels.join()!=='0,0,255,255,255,255,255,255,255,0,0,255,0,255,0,255'||gl.getError())
            throw Error('video flip/subimage '+pixels);
    "#,
    );
    frame(
        &host,
        true,
        &[
            0, 255, 255, 255, 0, 255, 255, 255, 0, 255, 255, 255, 0, 255, 255, 255,
        ],
    );
    check(
        &mut context,
        r#"
        gl.texSubImage2D(gl.TEXTURE_2D,0,0,0,gl.RGBA,gl.UNSIGNED_BYTE,video);
        gl.readPixels(0,0,2,2,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
        if (pixels.some((v,i)=>v!==[255,255,0,255][i%4])||gl.getError())
            throw Error('stale video frame');
    "#,
    );
}

#[test]
fn opaque_video_sources_throw_security_error_before_gpu_mutation() {
    let (mut context, host) = document();
    check(
        &mut context,
        r#"
        const video=document.createElement('video');document.body.append(video);
        const gl=new OffscreenCanvas(1,1).getContext('webgl2');
        const t=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,t);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,1,1,0,gl.RGBA,gl.UNSIGNED_BYTE,new Uint8Array([1,2,3,255]));
    "#,
    );
    frame(&host, false, SOURCE);
    check(
        &mut context,
        r#"
        video.crossOrigin='anonymous'; Object.defineProperty(video,'readyState',{value:4});
        for (const upload of [()=>gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,gl.RGBA,gl.UNSIGNED_BYTE,video),
            ()=>gl.texSubImage2D(gl.TEXTURE_2D,0,0,0,gl.RGBA,gl.UNSIGNED_BYTE,video)]) {
            let rejected=false;try{upload()}catch(e){rejected=e.name==='SecurityError'}
            if(!rejected)throw Error('opaque pixels escaped');
        }
        const fb=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,t,0);
        const pixel=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        if(pixel.join()!=='1,2,3,255'||gl.getError())throw Error('security rejection changed GPU');
    "#,
    );
}

#[test]
fn pixel_buffer_conflicts_precede_unavailable_and_opaque_dom_pixels() {
    let (mut context, host) = document();
    check(
        &mut context,
        r#"
        const video=document.createElement('video');document.body.append(video);
        const gl=new OffscreenCanvas(2,2).getContext('webgl2');
        gl.bindTexture(gl.TEXTURE_2D,gl.createTexture());
        const pbo=gl.createBuffer();gl.bindBuffer(gl.PIXEL_UNPACK_BUFFER,pbo);
        gl.bufferData(gl.PIXEL_UNPACK_BUFFER,64,gl.STATIC_DRAW);
        const upload=()=>gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,gl.RGBA,gl.UNSIGNED_BYTE,video);
        upload();if(gl.getError()!==gl.INVALID_OPERATION)throw Error('video source/PBO order');
    "#,
    );
    frame(&host, false, SOURCE);
    check(
        &mut context,
        r#"
        upload();if(gl.getError()!==gl.INVALID_OPERATION)throw Error('opaque source/PBO order');
        gl.bindBuffer(gl.PIXEL_UNPACK_BUFFER,null);
        let security=false;try{upload()}catch(e){security=e.name==='SecurityError'}
        if(!security)throw Error('origin violation ignored after unbinding');
    "#,
    );
}

#[test]
fn load_revokes_old_video_bytes_synchronously_and_forged_brands_are_rejected() {
    let (mut context, host) = document();
    check(
        &mut context,
        r#"
        const video=document.createElement('video');document.body.append(video);
        const gl=new OffscreenCanvas(2,2).getContext('webgl2');gl.bindTexture(gl.TEXTURE_2D,gl.createTexture());
    "#,
    );
    frame(&host, true, SOURCE);
    check(
        &mut context,
        r#"
        video.load();
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,gl.RGBA,gl.UNSIGNED_BYTE,video);
        if(gl.getError()!==gl.INVALID_VALUE)throw Error('load retained revoked pixels');
        let rejected=false;try{gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,gl.RGBA,gl.UNSIGNED_BYTE,
            Object.create(HTMLVideoElement.prototype))}catch(e){rejected=e instanceof TypeError}
        if(!rejected)throw Error('forged video brand');
    "#,
    );
}
