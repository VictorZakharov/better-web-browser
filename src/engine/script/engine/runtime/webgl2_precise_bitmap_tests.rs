//! Encoded ImageBitmap geometry, ownership and native source-precision contracts.
use super::webgl2_precise_image_tests::{context, encoded};
use super::*;

fn run(body: &str) {
    let (mut context, _host) = context();
    context.eval(Source::from_bytes(format!(r#"
        globalThis.precisionResult='pending';
        (async()=>{{{body}}})().then(()=>precisionResult='pass',error=>precisionResult=String(error));
    "#))).unwrap();
    context.run_jobs().unwrap();
    context
        .eval(Source::from_bytes(
            "if(precisionResult!=='pass')throw Error(precisionResult);",
        ))
        .unwrap();
}

const READ_RGB10: &str = r#"
    const gl=new OffscreenCanvas(8,8).getContext('webgl2',{antialias:false});
    const texture=gl.createTexture(),fb=gl.createFramebuffer();
    gl.bindTexture(gl.TEXTURE_2D,texture);gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
    const read=source=>{
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGB10_A2,gl.RGBA,gl.UNSIGNED_INT_2_10_10_10_REV,source);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        const words=new Uint32Array(source.width*source.height);
        gl.readPixels(0,0,source.width,source.height,gl.RGBA,gl.UNSIGNED_INT_2_10_10_10_REV,words);
        if(gl.getError()!==0)throw Error('native RGB10 bitmap read failed');
        return Array.from(words,word=>word&1023);
    };
    const expect=(actual,expected)=>{if(actual.join()!==expected.join())
        throw Error('precise bitmap pixels '+actual+' expected '+expected);};
"#;

#[test]
fn precise_bitmap_private_word_views_do_not_escape_through_data_view_hooks() {
    let bytes = encoded(
        8,
        1,
        (0..8).flat_map(|step| [step * 64, 0, 0, 65535]).collect(),
    );
    run(&format!(
        r#"
        {READ_RGB10}
        const originalGet=DataView.prototype.getUint16;
        const originalSet=DataView.prototype.setUint16;
        let escaped=0;
        DataView.prototype.getUint16=function(...args){{
            escaped++;return originalGet.apply(this,args);
        }};
        DataView.prototype.setUint16=function(...args){{
            escaped++;return originalSet.apply(this,args);
        }};
        let bitmap,copy,moved;
        try{{
            bitmap=await createImageBitmap(new Blob([new Uint8Array({bytes})]),{{premultiplyAlpha:'none'}});
            copy=structuredClone(bitmap);
            moved=structuredClone(bitmap,{{transfer:[bitmap]}});
        }}finally{{
            DataView.prototype.getUint16=originalGet;
            DataView.prototype.setUint16=originalSet;
        }}
        if(escaped!==0)throw Error('private word views escaped: '+escaped);
        if(bitmap.width!==0)throw Error('source was not detached');
        expect(read(copy),[0,1,2,3,4,5,6,7]);
        expect(read(moved),[0,1,2,3,4,5,6,7]);
        copy.close();moved.close();
    "#
    ));
}

#[test]
fn webgl2_precise_bitmap_blob_clone_transfer_and_peer_close_preserve_all_steps() {
    let bytes = encoded(
        8,
        1,
        (0..8).flat_map(|step| [step * 64, 0, 0, 65535]).collect(),
    );
    run(&format!(
        r#"
        {READ_RGB10}
        const bitmap=await createImageBitmap(new Blob([new Uint8Array({bytes})]),{{premultiplyAlpha:'none'}});
        const expected=[0,1,2,3,4,5,6,7];expect(read(bitmap),expected);
        const copies=structuredClone([bitmap,bitmap]);
        if(copies[0]!==copies[1] || copies[0]===bitmap)throw Error('clone identity');
        expect(read(copies[0]),expected);
        const moved=structuredClone(bitmap,{{transfer:[bitmap]}});
        if(bitmap.width!==0 || bitmap.height!==0)throw Error('transfer did not detach');
        bitmap.close();expect(read(moved),expected);expect(read(copies[0]),expected);
        const copied=await createImageBitmap(moved,{{premultiplyAlpha:'none'}});
        moved.close();expect(read(copied),expected);
        copied.close();copies[0].close();
    "#
    ));
}

#[test]
fn webgl2_precise_bitmap_crop_flip_and_transparent_padding_use_owned_word_geometry() {
    let bytes = encoded(
        4,
        2,
        (0..8).flat_map(|step| [step * 64, 0, 0, 65535]).collect(),
    );
    run(&format!(
        r#"
        {READ_RGB10}
        const image=__preciseImage({bytes});
        const flipped=await createImageBitmap(image,1,0,2,2,{{imageOrientation:'flipY',premultiplyAlpha:'none'}});
        expect(read(flipped),[5,6,1,2]);
        const padded=await createImageBitmap(image,-1,0,3,1,{{premultiplyAlpha:'none'}});
        expect(read(padded),[0,0,1]);
        const pixel=new Uint32Array(3);
        gl.readPixels(0,0,3,1,gl.RGBA,gl.UNSIGNED_INT_2_10_10_10_REV,pixel);
        if(pixel[0]!==0 || pixel[1]>>>30!==3 || pixel[2]>>>30!==3)throw Error('padding alpha');
        // An author field cannot replace internal words on a genuine bitmap.
        flipped.pixels16=new Uint16Array(16);expect(read(flipped),[5,6,1,2]);
        flipped.close();padded.close();
    "#
    ));
}

#[test]
fn webgl2_precise_bitmap_resize_filters_before_destination_quantization() {
    let bytes = encoded(
        8,
        1,
        (0..8).flat_map(|step| [step * 128, 0, 0, 65535]).collect(),
    );
    run(&format!(
        r#"
        {READ_RGB10}
        const image=__preciseImage({bytes});
        for(const resizeQuality of ['low','medium','high']){{
            const bitmap=await createImageBitmap(image,{{resizeWidth:4,resizeHeight:1,
                resizeQuality,premultiplyAlpha:'none'}});
            expect(read(bitmap),[1,5,9,13]);bitmap.close();
        }}
        const nearest=await createImageBitmap(image,{{resizeWidth:16,resizeHeight:1,
            resizeQuality:'pixelated',premultiplyAlpha:'none'}});
        expect(read(nearest),[0,0,2,2,4,4,6,6,8,8,10,10,12,12,14,14]);
        nearest.close();
    "#
    ));
}

#[test]
fn webgl2_precise_bitmap_alpha_is_applied_once_and_canvas_remains_a_byte_destination() {
    let bytes = encoded(1, 1, vec![32769, 16385, 8193, 257]);
    run(&format!(
        r#"
        const bitmap=await createImageBitmap(new Blob([new Uint8Array({bytes})]),{{premultiplyAlpha:'premultiply'}});
        const gl=new OffscreenCanvas(1,1).getContext('webgl2',{{antialias:false}});
        if(!gl.getExtension('EXT_color_buffer_float'))throw Error('native HDR extension');
        const texture=gl.createTexture(),fb=gl.createFramebuffer();
        gl.bindTexture(gl.TEXTURE_2D,texture);gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        // These WebGL pixel-store flags must not reformat ImageBitmap data.
        gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL,true);gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL,true);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA32F,gl.RGBA,gl.FLOAT,bitmap);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        const actual=new Float32Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.FLOAT,actual);
        const expected=[129,64,32,257].map(word=>word/65535);
        if(actual.some((value,index)=>Math.abs(value-expected[index])>1e-8)||gl.getError()!==0)
            throw Error('bitmap premultiplication '+actual);
        const straight=await createImageBitmap(bitmap,{{premultiplyAlpha:'none'}});
        bitmap.close();
        const canvas=new OffscreenCanvas(1,1),ctx=canvas.getContext('2d');ctx.drawImage(straight,0,0);
        const pixel=ctx.getImageData(0,0,1,1).data;
        if(pixel[3]!==1 || Math.abs(pixel[0]-128)>1)throw Error('byte Canvas destination '+pixel);
        const renderer=new OffscreenCanvas(1,1).getContext('bitmaprenderer');
        renderer.transferFromImageBitmap(straight);
        if(straight.width!==0)throw Error('bitmaprenderer did not detach precise storage');
    "#
    ));
}
