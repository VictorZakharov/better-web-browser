//! Client-view transforms select the correct rows and preserve actual GPU channels.
use super::webgl2_bindings_tests::{check, document};

#[test]
fn webgl2_view_transform_selects_padded_subrectangle_before_flipping() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=new OffscreenCanvas(2,2).getContext('webgl2');
        const texture=gl.createTexture(), fb=gl.createFramebuffer();
        gl.bindTexture(gl.TEXTURE_2D,texture); gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        const source=new Uint8Array(100).fill(99);
        // Four RGBA pixels per row. Skip one row/pixel and upload two rows
        // of two pixels. The ten-argument offset counts elements, not bytes.
        source.set([200,100,50,128,100,80,60,255],24);
        source.set([40,60,80,0,60,120,180,85],40);
        const before=[...source].join();
        gl.pixelStorei(gl.UNPACK_ALIGNMENT,8);
        gl.pixelStorei(gl.UNPACK_ROW_LENGTH,4);
        gl.pixelStorei(gl.UNPACK_SKIP_PIXELS,1);
        gl.pixelStorei(gl.UNPACK_SKIP_ROWS,1);
        gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL,true);
        gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL,true);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,2,2,0,gl.RGBA,gl.UNSIGNED_BYTE,source,4);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        const pixel=new Uint8Array(16);
        gl.readPixels(0,0,2,2,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        if ([...pixel].join()!=='0,0,0,0,20,40,60,85,100,50,25,128,100,80,60,255')
            throw Error('transformed subrectangle '+pixel);
        if ([...source].join()!==before) throw Error('caller pixels mutated');
        for (const [pname,value] of [[gl.UNPACK_ALIGNMENT,8],[gl.UNPACK_ROW_LENGTH,4],
            [gl.UNPACK_SKIP_PIXELS,1],[gl.UNPACK_SKIP_ROWS,1],
            [gl.UNPACK_FLIP_Y_WEBGL,true],[gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL,true]])
            if (gl.getParameter(pname)!==value) throw Error('unpack state changed');
        gl.texSubImage2D(gl.TEXTURE_2D,0,0,0,2,2,gl.RGBA,gl.UNSIGNED_BYTE,source,4);
        gl.readPixels(0,0,2,2,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        if ([...pixel].join()!=='0,0,0,0,20,40,60,85,100,50,25,128,100,80,60,255' || gl.getError()!==0)
            throw Error('subimage transforms differ');
    "#,
    );
}

#[test]
fn webgl2_view_transform_float_and_half_keep_hdr_premultiplication() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=new OffscreenCanvas(2,2).getContext('webgl2');
        gl.getExtension('EXT_color_buffer_float');
        const texture=gl.createTexture(),fb=gl.createFramebuffer();
        gl.bindTexture(gl.TEXTURE_2D,texture);gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL,true);
        gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL,true);
        for (const [internal,type,source] of [
            [gl.RGBA32F,gl.FLOAT,new Float32Array([4,-2,0.5,0.25,8,-4,1,0.5])],
            [gl.RGBA16F,gl.HALF_FLOAT,new Uint16Array(new Float16Array([4,-2,0.5,0.25,8,-4,1,0.5]).buffer)]]) {
            const before=[...source].join();
            gl.texImage2D(gl.TEXTURE_2D,0,internal,1,2,0,gl.RGBA,type,source);
            gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
            const pixels=new Float32Array(8);gl.readPixels(0,0,1,2,gl.RGBA,gl.FLOAT,pixels);
            if ([...pixels].join()!=='4,-2,0.5,0.5,1,-0.5,0.125,0.25') throw Error('HDR transformed '+pixels);
            if ([...source].join()!==before || gl.getError()!==0) throw Error('float upload mutation/error');
        }
    "#,
    );
}

#[test]
fn webgl2_view_transform_packed_rgba_respects_alpha_bit_domains() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=new OffscreenCanvas(2,2).getContext('webgl2');
        const texture=gl.createTexture(),fb=gl.createFramebuffer();
        gl.bindTexture(gl.TEXTURE_2D,texture);gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        gl.pixelStorei(gl.UNPACK_ALIGNMENT,1);
        gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL,true);
        gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL,true);
        for (const [internal,type,source,expected] of [
            [gl.RGBA4,gl.UNSIGNED_SHORT_4_4_4_4,new Uint16Array([0xffff,0xf84a]),[170,85,34,170]],
            [gl.RGB5_A1,gl.UNSIGNED_SHORT_5_5_5_1,new Uint16Array([0xffff,0xfffe]),[0,0,0,0]],
            [gl.RGB10_A2,gl.UNSIGNED_INT_2_10_10_10_REV,new Uint32Array([0xffffffff,0x7fffffff]),[85,85,85,85]]]) {
            const before=[...source].join();
            gl.texImage2D(gl.TEXTURE_2D,0,internal,1,2,0,gl.RGBA,type,source);
            gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
            const pixel=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
            if ([...pixel].join()!==expected.join()) throw Error('packed alpha '+type+': '+pixel);
            if ([...source].join()!==before || gl.getError()!==0) throw Error('packed upload mutation/error');
        }
    "#,
    );
}

#[test]
fn webgl2_view_transform_short_or_overlapping_input_preserves_storage() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=new OffscreenCanvas(2,2).getContext('webgl2');
        const texture=gl.createTexture(),fb=gl.createFramebuffer();
        gl.bindTexture(gl.TEXTURE_2D,texture);gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,1,1,0,gl.RGBA,gl.UNSIGNED_BYTE,new Uint8Array([255,0,0,255]));
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL,true);
        gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL,true);
        const expect=(invoke,error)=>{invoke();if(gl.getError()!==error || gl.getError()!==0)throw Error('validation order');};
        expect(()=>gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,1,2,0,gl.RGBA,gl.UNSIGNED_BYTE,new Uint8Array(7)),gl.INVALID_OPERATION);
        gl.pixelStorei(gl.UNPACK_ROW_LENGTH,1);
        expect(()=>gl.texSubImage2D(gl.TEXTURE_2D,0,0,0,2,1,gl.RGBA,gl.UNSIGNED_BYTE,new Uint8Array(8)),gl.INVALID_OPERATION);
        gl.pixelStorei(gl.UNPACK_ROW_LENGTH,0);
        const pixel=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        if ([...pixel].join()!=='255,0,0,255') throw Error('failed transform changed old image');
        // Null storage allocation ignores transform/skip constraints entirely.
        gl.pixelStorei(gl.UNPACK_SKIP_ROWS,1000);gl.pixelStorei(gl.UNPACK_SKIP_PIXELS,1000);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,1,1,0,gl.RGBA,gl.UNSIGNED_BYTE,null);
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        if ([...pixel].join()!=='0,0,0,0' || gl.getError()!==0) throw Error('null storage transform');
    "#,
    );
}
