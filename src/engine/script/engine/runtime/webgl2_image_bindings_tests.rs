//! DOM images select source subrectangles before tight native upload.
use super::webgl2_bindings_tests::{check, document};

#[test]
fn webgl2_realm_dom_cube_subuploads_validate_existing_face_format_without_mutating_storage() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(2,2));
        const texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_CUBE_MAP,texture);
        const face=gl.TEXTURE_CUBE_MAP_POSITIVE_X;
        const red=new ImageData(new Uint8ClampedArray([255,0,0,255]),1,1);
        const green=new ImageData(new Uint8ClampedArray([0,255,0,255]),1,1);
        // A framebuffer-attached cube image needs a cube-complete level (Chrome
        // also reports INCOMPLETE_ATTACHMENT when only one face is defined).
        for(let target=face;target<face+6;target++)
            gl.texImage2D(target,0,gl.RGB5_A1,gl.RGBA,gl.UNSIGNED_SHORT_5_5_5_1,red);
        gl.texSubImage2D(face,0,0,0,gl.RGBA,gl.UNSIGNED_SHORT_5_5_5_1,green);
        if(gl.getError()!==0) throw Error('cube DOM face subupload');
        // This packed type is valid for typed RGB5_A1 input but not for DOM input.
        gl.texSubImage2D(face,0,0,0,gl.RGBA,gl.UNSIGNED_INT_2_10_10_10_REV,red);
        if(gl.getError()!==gl.INVALID_OPERATION) throw Error('DOM table widened to GLES3 client-memory table');
        const fb=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,face,texture,0);
        const pixel=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        const error=gl.getError();
        if([...pixel].join()!=='0,255,0,255' || error!==0)
            throw Error('rejected DOM upload changed face: '+pixel+' error '+error+' status '+gl.checkFramebufferStatus(gl.FRAMEBUFFER));
    "#,
    );
}

#[test]
fn webgl2_realm_dom_unsigned_float_packing_matches_chrome_hdr_and_low_alpha_pixels() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(2,2));
        const extension=gl.getExtension('EXT_color_buffer_float');
        if (!extension || extension!==gl.getExtension('ext_color_buffer_float') ||
            Object.prototype.toString.call(extension)!=='[object EXT_color_buffer_float]')
            throw Error('core HDR extension negotiation');
        const texture=gl.createTexture(),fb=gl.createFramebuffer();
        gl.bindTexture(gl.TEXTURE_2D,texture); gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        for (const [rgba,premultiply,expected] of [
            [[123,45,67,255],false,[0.48046875,0.17578125,0.265625,1]],
            [[1,1,1,1],true,[0,0,0,1]],
            [[0,0,0,255],false,[0,0,0,1]],
            [[255,255,255,255],false,[1,1,1,1]]]) {
            const image=new ImageData(new Uint8ClampedArray(rgba),1,1);
            gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL,premultiply);
            gl.texImage2D(gl.TEXTURE_2D,0,gl.R11F_G11F_B10F,gl.RGB,gl.UNSIGNED_INT_10F_11F_11F_REV,image);
            gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
            if(gl.checkFramebufferStatus(gl.FRAMEBUFFER)!==gl.FRAMEBUFFER_COMPLETE)
                throw Error('packed unsigned-float target incomplete');
            const pixel=new Float32Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.FLOAT,pixel);
            if ([...pixel].some((value,index)=>value!==expected[index]) || gl.getError()!==0)
                throw Error('packed unsigned-float native values '+pixel);
        }
        const one=new OffscreenCanvas(1,1).getContext('webgl');
        if (one.getExtension('EXT_color_buffer_float')!==null) throw Error('WebGL2 HDR leaked into WebGL1');
    "#,
    );
}

#[test]
fn webgl2_realm_image_overloads_convert_platform_brands_even_when_lost() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(2,2));
        const image=new ImageData(new Uint8ClampedArray([10,20,30,255]),1,1);
        const texture=gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D,texture);
        Object.setPrototypeOf(image,null);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,gl.RGBA,gl.UNSIGNED_BYTE,image);
        const error=gl.getError();
        if (error!==0) throw Error('genuine source depends on mutable prototype: '+error);
        gl.getExtension('WEBGL_lose_context').loseContext();
        const forged=Object.create(ImageData.prototype);
        Object.assign(forged,{width:1,height:1,data:new Uint8ClampedArray(4)});
        let converted=[];
        try {
            gl.texImage2D({valueOf(){converted.push('target');return gl.TEXTURE_2D;}},
                0,gl.RGBA8,gl.RGBA,gl.UNSIGNED_BYTE,forged);
            throw Error('forged source was accepted by lost context');
        } catch(error) { if (!(error instanceof TypeError)) throw error; }
        if (converted.join()!=='target') throw Error('image overload conversion order');
        for (const source of [Object.create(OffscreenCanvas.prototype),
            Object.create(HTMLCanvasElement.prototype),Object.create(HTMLImageElement.prototype),null,{}]) {
            try { gl.texSubImage2D(gl.TEXTURE_2D,0,0,0,gl.RGBA,gl.UNSIGNED_BYTE,source);
                throw Error('invalid source accepted during context loss'); }
            catch(error) { if (!(error instanceof TypeError)) throw error; }
        }
        // Valid detached ImageData passes IDL branding. Lost-context no-op occurs
        // before checking whether its bitmap can still be uploaded.
        structuredClone(image.data.buffer,{transfer:[image.data.buffer]});
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,gl.RGBA,gl.UNSIGNED_BYTE,image);
    "#,
    );
}

#[test]
fn webgl2_realm_dom_packed_color_formats_preserve_real_native_channels() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(2,2));
        const image=new ImageData(new Uint8ClampedArray([255,0,255,255]),1,1);
        const texture=gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D,texture);
        const fb=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        for (const [internal,format,type] of [
            [gl.RGB565,gl.RGB,gl.UNSIGNED_SHORT_5_6_5],
            [gl.RGBA4,gl.RGBA,gl.UNSIGNED_SHORT_4_4_4_4],
            [gl.RGB5_A1,gl.RGBA,gl.UNSIGNED_SHORT_5_5_5_1],
            [gl.RGB10_A2,gl.RGBA,gl.UNSIGNED_INT_2_10_10_10_REV]]) {
            gl.texImage2D(gl.TEXTURE_2D,0,internal,format,type,image);
            gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
            if (gl.checkFramebufferStatus(gl.FRAMEBUFFER)!==gl.FRAMEBUFFER_COMPLETE)
                throw Error('packed DOM texture not renderable');
            const pixel=new Uint8Array(4);
            gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
            if ([...pixel].join()!=='255,0,255,255' || gl.getError()!==0)
                throw Error('packed DOM channel conversion '+type+' '+pixel);
            gl.texSubImage2D(gl.TEXTURE_2D,0,0,0,format,type,image);
            if (gl.getError()!==0) throw Error('packed DOM subupload');
        }
    "#,
    );
}

#[test]
fn webgl2_realm_dom_packed_quantization_matches_hidden_chromium_pixels() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(2,2));
        const texture=gl.createTexture(),fb=gl.createFramebuffer();
        gl.bindTexture(gl.TEXTURE_2D,texture); gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        const image=new ImageData(new Uint8ClampedArray([123,45,67,128]),1,1);
        // Project-owned synthetic ImageData, measured with unified-headless Chrome.
        const rows=[
            [gl.RGB565,gl.RGB,gl.UNSIGNED_SHORT_5_6_5,[123,45,66,255],[58,20,33,255]],
            [gl.RGBA4,gl.RGBA,gl.UNSIGNED_SHORT_4_4_4_4,[119,34,68,136],[51,17,34,136]],
            [gl.RGB5_A1,gl.RGBA,gl.UNSIGNED_SHORT_5_5_5_1,[123,41,66,255],[58,16,33,255]],
            [gl.RGB10_A2,gl.RGBA,gl.UNSIGNED_INT_2_10_10_10_REV,[123,45,67,85],[62,23,34,85]]];
        for (const premultiply of [false,true]) for (const [internal,format,type,straight,multiplied] of rows) {
            gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL,premultiply);
            gl.texImage2D(gl.TEXTURE_2D,0,internal,format,type,image);
            gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
            const pixel=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
            if ([...pixel].join()!==(premultiply?multiplied:straight).join() || gl.getError()!==0)
                throw Error('packed conversion differs from Chrome '+type+' '+pixel);
        }
    "#,
    );
}

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
