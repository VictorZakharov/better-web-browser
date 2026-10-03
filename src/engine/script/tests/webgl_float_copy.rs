use super::webgl_float_sampling::DRAW;
use super::webgl_instancing::check;

#[test]
fn webgl_float_framebuffer_copy_normalizes_all_legacy_formats_and_preserves_pixel_store() {
    check(&format!(
        r#"{DRAW}
        assert(gl.getExtension('OES_texture_float'),'float support');
        const half=gl.getExtension('OES_texture_half_float');assert(half,'half support');
        const source=gl.getParameter(gl.TEXTURE_BINDING_2D);
        const framebuffer=gl.createFramebuffer();
        const destination=gl.createTexture();
        const cases=[
            [gl.RGBA,[255,0,128,64]],
            [gl.RGB,[255,0,128,255]],
            [gl.ALPHA,[0,0,0,64]],
            [gl.LUMINANCE,[255,255,255,255]],
            [gl.LUMINANCE_ALPHA,[255,255,255,64]]
        ];
        for(const type of [gl.FLOAT,half.HALF_FLOAT_OES]){{
            gl.bindTexture(gl.TEXTURE_2D,source);
            const values=[2,-1,.5,.25];
            const data=type===gl.FLOAT?new Float32Array(values):new Uint16Array(new Float16Array(values).buffer);
            gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,1,1,0,gl.RGBA,type,data);
            gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);
            gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,source,0);
            assert(gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE,'source complete');
            error(0,'source allocation');
            for(const [format,expected] of cases){{
                gl.bindTexture(gl.TEXTURE_2D,destination);
                gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);
                gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);
                // Replacing a private floating ALPHA image must reset its swizzle.
                gl.texImage2D(gl.TEXTURE_2D,0,gl.ALPHA,1,1,0,gl.ALPHA,gl.FLOAT,new Float32Array([.9]));
                gl.pixelStorei(gl.PACK_ALIGNMENT,8);gl.pixelStorei(gl.UNPACK_ALIGNMENT,8);
                gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);
                gl.copyTexImage2D(gl.TEXTURE_2D,0,format,0,0,1,1,0);
                error(0,'copy floating framebuffer '+format+' '+type);
                assert(gl.getParameter(gl.PACK_ALIGNMENT)===8,'pack state restored');
                assert(gl.getParameter(gl.UNPACK_ALIGNMENT)===8,'unpack state restored');
                gl.bindFramebuffer(gl.FRAMEBUFFER,null);
                const actual=draw();assert(near(actual,expected),'copied channels '+format+' '+actual);
            }}
            gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);
            gl.copyTexImage2D(gl.TEXTURE_2D,0,gl.RGBA,8,8,1,1,0);
            error(0,'out of bounds copy');gl.bindFramebuffer(gl.FRAMEBUFFER,null);
            assert(near(draw(),[0,0,0,0]),'outside source is securely initialized');
        }}
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_upload_border_and_zero_sized_null_subupload_are_validated() {
    check(&format!(
        r#"{DRAW}
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,1,1,1,gl.RGBA,gl.UNSIGNED_BYTE,null);
        error(gl.INVALID_VALUE,'nonzero border');
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,1,1,0,gl.RGBA,gl.UNSIGNED_BYTE,null);
        error(0,'valid image');
        gl.texSubImage2D(gl.TEXTURE_2D,0,0,0,0,0,gl.RGBA,gl.UNSIGNED_BYTE,null);
        error(gl.INVALID_VALUE,'null subupload even at zero size');
        gl.texSubImage2D(gl.TEXTURE_2D,0,0,0,0,0,gl.RGBA,gl.UNSIGNED_BYTE,new Uint8Array(0));
        error(0,'empty nonnull zero sized subupload');
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_float_subcopies_preserve_destination_border_and_pixel_store() {
    check(&format!(
        r#"{DRAW}
        gl.getExtension('OES_texture_float');
        const source=gl.createTexture(), framebuffer=gl.createFramebuffer();
        const destination=gl.getParameter(gl.TEXTURE_BINDING_2D);
        gl.bindTexture(gl.TEXTURE_2D,source);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,1,1,0,gl.RGBA,gl.FLOAT,new Float32Array([2,-1,.5,.25]));
        gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,source,0);
        assert(gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE,'float source');
        const cases=[
            [gl.RGBA,[255,0,128,64]], [gl.RGB,[255,0,128,255]],
            [gl.ALPHA,[0,0,0,64]], [gl.LUMINANCE,[255,255,255,255]],
            [gl.LUMINANCE_ALPHA,[255,255,255,64]]
        ];
        for(const [format,expected] of cases){{
            gl.bindTexture(gl.TEXTURE_2D,destination);
            gl.pixelStorei(gl.UNPACK_ALIGNMENT,1);
            gl.texImage2D(gl.TEXTURE_2D,0,format,1,1,0,format,gl.UNSIGNED_BYTE,null);
            gl.pixelStorei(gl.PACK_ALIGNMENT,8);gl.pixelStorei(gl.UNPACK_ALIGNMENT,8);
            gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);
            gl.copyTexSubImage2D(gl.TEXTURE_2D,0,0,0,0,0,1,1);
            error(0,'floating subcopy '+format);
            assert(gl.getParameter(gl.PACK_ALIGNMENT)===8,'pack preserved');
            assert(gl.getParameter(gl.UNPACK_ALIGNMENT)===8,'unpack preserved');
            gl.copyTexSubImage2D(gl.TEXTURE_2D,0,1,0,0,0,1,1);
            error(gl.INVALID_VALUE,'outside destination');
            assert(gl.getParameter(gl.UNPACK_ALIGNMENT)===8,'failed copy unpack preserved');
            gl.bindFramebuffer(gl.FRAMEBUFFER,null);
            assert(near(draw(),expected),'subcopy channels '+format);
        }}
        document.querySelector('output').textContent='pass';
    "#
    ));
}
