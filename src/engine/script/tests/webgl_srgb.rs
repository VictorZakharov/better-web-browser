use super::webgl_float_sampling::DRAW;
use super::webgl_instancing::check;

#[test]
fn webgl_srgb_types_queries_and_mips_are_extension_gated() {
    check(&format!(
        r#"{DRAW}
        gl.texImage2D(gl.TEXTURE_2D,0,0x8c42,1,1,0,0x8c42,gl.UNSIGNED_BYTE,null);
        error(gl.INVALID_ENUM,'sRGB disabled');
        const ext=gl.getExtension('EXT_sRGB');assert(ext,'sRGB capability');
        assert(Object.prototype.toString.call(ext)==='[object EXT_sRGB]','brand');
        assert(typeof EXT_sRGB==='undefined','no constructor');
        assert(ext===gl.getExtension('ext_SRGB'),'cached identity');
        assert(ext.SRGB_EXT===0x8c40&&ext.SRGB_ALPHA_EXT===0x8c42&&ext.SRGB8_ALPHA8_EXT===0x8c43&&ext.FRAMEBUFFER_ATTACHMENT_COLOR_ENCODING_EXT===0x8210,'constants');
        gl.texImage2D(gl.TEXTURE_2D,0,ext.SRGB_ALPHA_EXT,1,1,0,ext.SRGB_ALPHA_EXT,gl.FLOAT,null);
        error(gl.INVALID_OPERATION,'byte-only type');
        gl.texImage2D(gl.TEXTURE_2D,0,ext.SRGB8_ALPHA8_EXT,1,1,0,ext.SRGB8_ALPHA8_EXT,gl.UNSIGNED_BYTE,null);
        error(gl.INVALID_ENUM,'sized renderbuffer token not texture format');
        gl.texImage2D(gl.TEXTURE_2D,0,ext.SRGB_ALPHA_EXT,1,1,0,ext.SRGB_ALPHA_EXT,gl.UNSIGNED_BYTE,new Uint8Array([127,127,127,64]));
        error(0,'sRGB definition');gl.generateMipmap(gl.TEXTURE_2D);
        error(gl.INVALID_OPERATION,'WebGL1 sRGB mips forbidden');
        const peer=document.createElement('canvas').getContext('webgl');
        peer.bindTexture(peer.TEXTURE_2D,peer.createTexture());
        peer.texImage2D(peer.TEXTURE_2D,0,ext.SRGB_ALPHA_EXT,1,1,0,ext.SRGB_ALPHA_EXT,peer.UNSIGNED_BYTE,null);
        assert(peer.getError()===peer.INVALID_ENUM,'peer capability independent');
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_srgb_sampling_decodes_color_but_not_alpha_and_subuploads_preserve_the_format() {
    check(&format!(
        r#"{DRAW}
        const ext=gl.getExtension('EXT_sRGB');assert(ext,'sRGB capability');
        gl.pixelStorei(gl.UNPACK_ALIGNMENT,1);
        for(const format of [ext.SRGB_EXT,ext.SRGB_ALPHA_EXT]){{
            for(const [encoded,linear] of [[0,0],[63,13],[127,54],[191,133],[255,255]]){{
                const bytes=new Uint8Array(format===ext.SRGB_EXT?[encoded,encoded,encoded]:[encoded,encoded,encoded,64]);
                gl.texImage2D(gl.TEXTURE_2D,0,format,1,1,0,format,gl.UNSIGNED_BYTE,bytes);
                error(0,'sRGB upload');
                assert(near(draw(),[linear,linear,linear,format===ext.SRGB_EXT?255:64]),'sRGB decode '+format+' '+encoded);
            }}
        }}
        gl.texSubImage2D(gl.TEXTURE_2D,0,0,0,1,1,ext.SRGB_ALPHA_EXT,gl.UNSIGNED_BYTE,new Uint8Array([127,127,127,128]));
        error(0,'sRGB subupload');assert(near(draw(),[54,54,54,128]),'updated encoded texel');
        gl.texSubImage2D(gl.TEXTURE_2D,0,0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,new Uint8Array([0,0,0,0]));
        error(gl.INVALID_OPERATION,'linear alias cannot subupload to sRGB');
        assert(near(draw(),[54,54,54,128]),'rejection preserves storage');
        const image=new OffscreenCanvas(1,1).getContext('2d');
        image.fillStyle='rgb(127,127,127)';image.fillRect(0,0,1,1);
        gl.texImage2D(gl.TEXTURE_2D,0,ext.SRGB_ALPHA_EXT,ext.SRGB_ALPHA_EXT,gl.UNSIGNED_BYTE,image.canvas);
        error(0,'DOM source sRGB upload');assert(near(draw(),[54,54,54,255]),'DOM bytes decoded once');
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_srgb_framebuffer_storage_encodes_shader_output_and_blends_in_linear_space() {
    check(&format!(
        r#"{DRAW}
        const ext=gl.getExtension('EXT_sRGB');assert(ext,'sRGB capability');
        const source=gl.getParameter(gl.TEXTURE_BINDING_2D);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,1,1,0,gl.RGBA,gl.UNSIGNED_BYTE,new Uint8Array([64,64,64,255]));
        const fbo=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,fbo);
        const renderbuffer=gl.createRenderbuffer();gl.bindRenderbuffer(gl.RENDERBUFFER,renderbuffer);
        gl.renderbufferStorage(gl.RENDERBUFFER,ext.SRGB8_ALPHA8_EXT,8,4);
        gl.framebufferRenderbuffer(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.RENDERBUFFER,renderbuffer);
        error(0,'sRGB renderbuffer');assert(gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE,'complete encoded target');
        assert(gl.getFramebufferAttachmentParameter(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,ext.FRAMEBUFFER_ATTACHMENT_COLOR_ENCODING_EXT)===ext.SRGB_EXT,'sRGB encoding reflected');
        gl.clearColor(0,0,0,0);gl.clear(gl.COLOR_BUFFER_BIT);
        gl.enable(gl.BLEND);gl.blendFunc(gl.ONE,gl.ONE);
        gl.drawArrays(gl.TRIANGLE_STRIP,0,4);gl.drawArrays(gl.TRIANGLE_STRIP,0,4);
        error(0,'two additive linear draws');
        const pixel=new Uint8Array(4);gl.readPixels(1,1,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        error(0,'encoded read');assert([...pixel].slice(0,3).every(v=>Math.abs(v-188)<=2),'linear blend encoded once '+pixel);
        gl.disable(gl.BLEND);
        const encoded=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,encoded);
        gl.texImage2D(gl.TEXTURE_2D,0,ext.SRGB_EXT,8,4,0,ext.SRGB_EXT,gl.UNSIGNED_BYTE,null);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,encoded,0);
        assert(gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_INCOMPLETE_ATTACHMENT,'sRGB RGB image is not color renderable');
        assert(gl.getFramebufferAttachmentParameter(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,ext.FRAMEBUFFER_ATTACHMENT_COLOR_ENCODING_EXT)===ext.SRGB_EXT,'encoding even on unrenderable RGB image');
        gl.bindFramebuffer(gl.FRAMEBUFFER,null);gl.bindTexture(gl.TEXTURE_2D,source);
        assert(near(draw(),[64,64,64,255]),'default target remains linear');
        document.querySelector('output').textContent='pass';
    "#
    ));
}
