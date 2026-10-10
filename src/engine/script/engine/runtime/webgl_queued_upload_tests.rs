//! Public uploads retain call-time snapshots even when the native owner is deferred.
use super::webgl_owned_copy_tests::both;

#[test]
fn queued_texture_views_keep_byte_offsets_and_call_time_ownership_in_both_realms() {
    both(
        r#"
        const gl=new OffscreenCanvas(2,1).getContext('webgl2',{antialias:false});
        const texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,texture);
        const initial=new Uint8Array([99,255,0,0,255,0,255,0,255,98]);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,2,1,0,gl.RGBA,gl.UNSIGNED_BYTE,initial,1);
        initial.fill(0);initial.buffer.transfer();
        const update=new Uint8Array([99,0,0,255,255,98]);
        gl.texSubImage2D(gl.TEXTURE_2D,0,1,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,update,1);
        update.fill(17);update.buffer.transfer();
        const framebuffer=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        const pixels=new Uint8Array(8);gl.readPixels(0,0,2,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
        if(String(pixels)!=='255,0,0,255,0,0,255,255'||gl.getError()!==0)throw Error('queued texture snapshot');
    "#,
    );
}

#[test]
fn queued_volume_uploads_snapshot_shared_sources_and_preserve_later_binding_order() {
    both(
        r#"
        const gl=new OffscreenCanvas(1,1).getContext('webgl2',{antialias:false});
        const texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D_ARRAY,texture);
        const source=new Uint8Array(new SharedArrayBuffer(8));source.set([255,0,0,255,0,255,0,255]);
        gl.texImage3D(gl.TEXTURE_2D_ARRAY,0,gl.RGBA8,1,1,2,0,gl.RGBA,gl.UNSIGNED_BYTE,source);
        source.fill(0);
        const update=new Uint8Array([0,0,255,255]);
        gl.texSubImage3D(gl.TEXTURE_2D_ARRAY,0,0,0,1,1,1,1,gl.RGBA,gl.UNSIGNED_BYTE,update);
        update.fill(0);update.buffer.transfer();
        gl.bindTexture(gl.TEXTURE_2D_ARRAY,null);
        const framebuffer=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);
        const pixel=new Uint8Array(4);
        for(const [layer,expected]of [[0,'255,0,0,255'],[1,'0,0,255,255']]){
            gl.framebufferTextureLayer(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,texture,0,layer);
            gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
            if(String(pixel)!==expected)throw Error('queued volume layer '+layer+': '+pixel);
        }
        if(gl.getError()!==0)throw Error('volume upload GL error');
    "#,
    );
}

#[test]
fn queued_buffer_upload_and_pixel_unpack_read_observe_one_ordered_snapshot() {
    both(
        r#"
        const gl=new OffscreenCanvas(1,1).getContext('webgl2',{antialias:false});
        const buffer=gl.createBuffer();gl.bindBuffer(gl.PIXEL_UNPACK_BUFFER,buffer);
        const source=new Uint8Array([255,0,0,255,0,255,0,255]);
        gl.bufferData(gl.PIXEL_UNPACK_BUFFER,source,gl.DYNAMIC_DRAW);source.fill(0);
        const update=new Uint8Array([0,0,255,255]);
        gl.bufferSubData(gl.PIXEL_UNPACK_BUFFER,4,update);update.fill(0);
        const texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,texture);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,1,1,0,gl.RGBA,gl.UNSIGNED_BYTE,4);
        gl.bindBuffer(gl.PIXEL_UNPACK_BUFFER,null);
        const framebuffer=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        const pixel=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        if(String(pixel)!=='0,0,255,255'||gl.getError()!==0)throw Error('queued PBO snapshot ordering');
    "#,
    );
}
