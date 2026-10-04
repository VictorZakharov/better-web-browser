//! Real encoded source -> trusted decoder snapshot -> native texture readback.
//! Test-only bootstrap seam bypasses network I/O, not decoding or GPU operations.
use super::*;
use crate::engine::script::{HostState, bootstrap, module_loader::WebModuleLoader};
use image::{ImageBuffer, ImageFormat, Rgba};
use std::io::Cursor;

pub(super) fn context() -> (Context, Rc<RefCell<HostState>>) {
    let host = Rc::new(RefCell::new(HostState::new(
        crate::engine::dom::parse("<!doctype html><body>").document,
        "https://example.test/",
        "UTF-8",
        Rc::new(WebModuleLoader::new()),
    )));
    let mut context = Context::new(HostBridge::Document(Rc::downgrade(&host))).unwrap();
    let marker = "const cancelDetachedImage = element => {";
    let bootstrap = bootstrap::BROWSER_BOOTSTRAP.replace(
        marker,
        r#"
        globalThis.__preciseImage = bytes => {
            const decoded=host('canvasDecode',new Uint8Array(bytes),false,false,true);
            if(!decoded) throw Error('real encoded source failed decoding');
            const image=new Image();
            detachedImageLoads.set(image,{decoded:{width:decoded[0],height:decoded[1],
                pixels:decoded[2],pixels16:decoded[3]?new Uint16Array(decoded[3].buffer,
                    decoded[3].byteOffset,decoded[3].byteLength/2):null}});
            updateImageElementState(image,true,decoded[0],decoded[1]);
            return image;
        };
        const cancelDetachedImage = element => {
    "#,
    );
    assert_ne!(bootstrap, bootstrap::BROWSER_BOOTSTRAP);
    context.eval(Source::from_bytes(&bootstrap)).unwrap();
    (context, host)
}

pub(super) fn encoded(width: u32, height: u32, words: Vec<u16>) -> String {
    let image = ImageBuffer::<Rgba<u16>, _>::from_raw(width, height, words).unwrap();
    let mut output = Cursor::new(Vec::new());
    image.write_to(&mut output, ImageFormat::Png).unwrap();
    serde_json::to_string(&output.into_inner()).unwrap()
}

#[test]
fn webgl2_precise_image_rgb10_upload_keeps_sub_byte_steps_through_native_readback() {
    let (mut context, _host) = context();
    let bytes = encoded(
        8,
        1,
        (0..8).flat_map(|step| [step * 64, 0, 0, 65535]).collect(),
    );
    context
        .eval(Source::from_bytes(format!(
            r#"
        const image=__preciseImage({bytes});
        // Writable author properties are not authoritative decoded pixels.
        image.pixels16=new Uint16Array(32); image.pixels=new Uint8Array(32);
        const gl=new OffscreenCanvas(8,1).getContext('webgl2',{{antialias:false}});
        const texture=gl.createTexture(),fb=gl.createFramebuffer();
        gl.bindTexture(gl.TEXTURE_2D,texture);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGB10_A2,gl.RGBA,gl.UNSIGNED_INT_2_10_10_10_REV,image);
        gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        const words=new Uint32Array(8);
        gl.readPixels(0,0,8,1,gl.RGBA,gl.UNSIGNED_INT_2_10_10_10_REV,words);
        if(gl.getError()!==0 || new Set(words).size!==8)
            throw Error('precision collapsed: '+words);
        for(let step=0;step<8;step++)
            if((words[step]&1023)!==step || words[step]>>>30!==3)
                throw Error('normalized packed value '+step+': '+words[step]);
        gl.texSubImage2D(gl.TEXTURE_2D,0,0,0,gl.RGBA,gl.UNSIGNED_INT_2_10_10_10_REV,image);
        gl.readPixels(0,0,8,1,gl.RGBA,gl.UNSIGNED_INT_2_10_10_10_REV,words);
        if(new Set(words).size!==8 || gl.getError()!==0) throw Error('subupload precision');
    "#
        )))
        .unwrap();
}

#[test]
fn webgl2_precise_image_float_volume_uploads_apply_flip_skip_and_alpha_at_source_precision() {
    let (mut context, _host) = context();
    let bytes = encoded(
        2,
        2,
        vec![
            1, 65, 129, 32769, 257, 513, 769, 65535, 1025, 2049, 4097, 16385, 8193, 16385, 32769,
            65534,
        ],
    );
    context.eval(Source::from_bytes(format!(r#"
        const image=__preciseImage({bytes});
        const gl=new OffscreenCanvas(2,2).getContext('webgl2',{{antialias:false}});
        if(!gl.getExtension('EXT_color_buffer_float')) throw Error('real HDR extension unavailable');
        const texture=gl.createTexture(),fb=gl.createFramebuffer();
        gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        gl.bindTexture(gl.TEXTURE_3D,texture);
        gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL,true);
        gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL,true);
        gl.pixelStorei(gl.UNPACK_SKIP_PIXELS,1);
        gl.pixelStorei(gl.UNPACK_IMAGE_HEIGHT,1);
        gl.texImage3D(gl.TEXTURE_3D,0,gl.RGBA32F,1,1,2,0,gl.RGBA,gl.FLOAT,image);
        const values=new Float32Array(4);
        for(let layer=0;layer<2;layer++){{
            gl.framebufferTextureLayer(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,texture,0,layer);
            gl.readPixels(0,0,1,1,gl.RGBA,gl.FLOAT,values);
            const source=layer===0?[8193,16385,32769,65534]:[257,513,769,65535];
            for(let channel=0;channel<4;channel++){{
                const expected=source[channel]/65535*(channel===3?1:source[3]/65535);
                if(Math.abs(values[channel]-expected)>1e-7)
                    throw Error('precise volume/alpha channel '+layer+'/'+channel+': '+values);
            }}
        }}
        if(gl.getError()!==0 || gl.getParameter(gl.UNPACK_SKIP_PIXELS)!==1 ||
            gl.getParameter(gl.UNPACK_IMAGE_HEIGHT)!==1) throw Error('unpack state changed');
    "#))).unwrap();
}

#[test]
fn webgl2_precise_image_bytes_use_narrowed_channels_and_do_not_enable_forbidden_dom_formats() {
    let (mut context, _host) = context();
    let bytes = encoded(1, 1, vec![257, 32769, 65534, 65535]);
    context.eval(Source::from_bytes(format!(r#"
        const image=__preciseImage({bytes});
        const gl=new OffscreenCanvas(1,1).getContext('webgl2',{{antialias:false}});
        const texture=gl.createTexture(),fb=gl.createFramebuffer();
        gl.bindTexture(gl.TEXTURE_2D,texture);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,gl.RGBA,gl.UNSIGNED_BYTE,image);
        gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        const pixel=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        if([...pixel].join()!=='1,128,255,255' || gl.getError()!==0) throw Error('byte view '+pixel);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA16UI,gl.RGBA_INTEGER,gl.UNSIGNED_SHORT,image);
        if(gl.getError()!==gl.INVALID_OPERATION) throw Error('DOM upload table broadened');
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        if([...pixel].join()!=='1,128,255,255' || gl.getError()!==0) throw Error('rejected upload mutated storage');
        const forged=Object.create(HTMLImageElement.prototype);
        forged.pixels16=new Uint16Array([1,2,3,4]);
        try{{gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA32F,gl.RGBA,gl.FLOAT,forged);
            throw Error('forged image accepted');}}catch(error){{if(!(error instanceof TypeError))throw error;}}
    "#))).unwrap();
}
