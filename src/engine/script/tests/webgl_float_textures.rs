use super::webgl_instancing::check;

pub(super) const HELPERS: &str = r#"
    const gl=document.querySelector('canvas').getContext('webgl',{preserveDrawingBuffer:true});
    const assert=(v,s)=>{if(!v)throw Error(s)};
    const error=(code,label)=>{const actual=gl.getError();assert(actual===code,label+' actual '+actual+' expected '+code)};
    const near=(a,b)=>a.length===b.length&&a.every((v,i)=>Math.abs(v-b[i])<.002);
    const texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,texture);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);
    const attach=()=>{
        const f=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,f);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        const status=gl.checkFramebufferStatus(gl.FRAMEBUFFER);
        assert(status===gl.FRAMEBUFFER_COMPLETE,'HDR target complete actual '+status+' error '+gl.getError());
        return f;
    };
    const read=()=>{const out=new Float32Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.FLOAT,out);error(0,'float read');return [...out];};
"#;

#[test]
fn webgl_incomplete_read_queries_and_reads_have_distinct_errors() {
    check(&format!(
        r#"{HELPERS}
        gl.bindFramebuffer(gl.FRAMEBUFFER,gl.createFramebuffer());
        assert(gl.getParameter(gl.IMPLEMENTATION_COLOR_READ_FORMAT)===null,'invalid format query');
        error(gl.INVALID_OPERATION,'incomplete format query');
        assert(gl.getParameter(gl.IMPLEMENTATION_COLOR_READ_TYPE)===null,'invalid type query');
        error(gl.INVALID_OPERATION,'incomplete type query');
        const pixels=new Uint8Array([9,8,7,6]);
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
        error(gl.INVALID_FRAMEBUFFER_OPERATION,'incomplete actual read');
        assert([...pixels].join()==='9,8,7,6','invalid read leaves bytes unchanged');
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_float_upload_and_readback_preserve_hdr_values_and_destination_offsets() {
    check(&format!(
        r#"{HELPERS}
        const ext=gl.getExtension('OES_texture_float');assert(ext,'float textures');
        assert(ext===gl.getExtension('oes_TEXTURE_FLOAT'),'cached identity');
        assert(Object.prototype.toString.call(ext)==='[object OES_texture_float]','brand');
        assert(typeof OES_texture_float==='undefined','no interface constructor');
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,1,1,0,gl.RGBA,gl.FLOAT,new Float32Array([2,-1,.5,1]));
        error(0,'float upload');attach();
        assert(gl.getParameter(gl.IMPLEMENTATION_COLOR_READ_TYPE)===gl.FLOAT,'floating implementation read type');
        assert(gl.getParameter(gl.IMPLEMENTATION_COLOR_READ_FORMAT)===gl.RGBA,'floating implementation read format');
        assert(near(read(),[2,-1,.5,1]),'values not normalized or clamped');
        const destination=new Float32Array([91,92,93,94,95,96,97,98]);
        gl.readPixels(0,0,1,1,gl.RGBA,gl.FLOAT,destination.subarray(2,6));
        assert(near([...destination],[91,92,2,-1,.5,1,97,98]),'destination view range');
        error(0,'offset read');
        const bytes=new Uint8Array([9,9,9,9]);
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,bytes);
        error(gl.INVALID_OPERATION,'normalized read not allowed for floating target');
        assert([...bytes].join()==='9,9,9,9','failed read leaves destination untouched');
        const color=gl.getExtension('WEBGL_color_buffer_float');assert(color,'implicit color float');
        assert(gl.getFramebufferAttachmentParameter(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,color.FRAMEBUFFER_ATTACHMENT_COMPONENT_TYPE_EXT)===gl.FLOAT,'component type');
        gl.bindFramebuffer(gl.FRAMEBUFFER,null);
        assert(gl.getParameter(gl.IMPLEMENTATION_COLOR_READ_TYPE)===gl.UNSIGNED_BYTE,'default read type restored');
        gl.readPixels(0,0,1,1,gl.RGBA,gl.FLOAT,destination.subarray(2,6));
        error(gl.INVALID_OPERATION,'float read not allowed for default normalized target');
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_half_upload_uses_binary16_bits_and_float_readback() {
    check(&format!(
        r#"{HELPERS}
        const half=gl.getExtension('OES_texture_half_float');assert(half,'half texture');
        assert(half.HALF_FLOAT_OES===0x8d61,'type constant');
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,1,1,0,gl.RGBA,half.HALF_FLOAT_OES,
            new Uint16Array([0x4000,0xbc00,0x3800,0x3c00]));
        error(0,'half upload');attach();assert(near(read(),[2,-1,.5,1]),'binary16 values');
        const color=gl.getExtension('EXT_color_buffer_half_float');assert(color,'half renderability');
        assert(color.RGBA16F_EXT===0x881a&&color.RGB16F_EXT===0x881b,'storage constants');
        assert(gl.getFramebufferAttachmentParameter(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,color.FRAMEBUFFER_ATTACHMENT_COMPONENT_TYPE_EXT)===gl.FLOAT,'half component is float');
        gl.readPixels(0,0,1,1,gl.RGBA,half.HALF_FLOAT_OES,new Uint16Array(4));
        error(gl.INVALID_OPERATION,'half cannot replace mandatory FLOAT readback');
        gl.getFramebufferAttachmentParameter(gl.FRAMEBUFFER,gl.DEPTH_STENCIL_ATTACHMENT,color.FRAMEBUFFER_ATTACHMENT_COMPONENT_TYPE_EXT);
        error(gl.INVALID_OPERATION,'combined attachment component query');
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_float_and_half_dom_uploads_reuse_origin_clean_conversion() {
    check(&format!(
        r#"{HELPERS}
        const source=new ImageData(new Uint8ClampedArray([64,128,192,128]),1,1);
        assert(gl.getExtension('OES_texture_float'),'float capability');
        const half=gl.getExtension('OES_texture_half_float');assert(half,'half capability');
        const framebuffer=gl.createFramebuffer();
        for (const type of [gl.FLOAT,half.HALF_FLOAT_OES]) {{
            gl.bindFramebuffer(gl.FRAMEBUFFER,null);
            gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL,false);
            gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,gl.RGBA,type,source);
            error(0,'DOM texture upload');
            gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);
            gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
            assert(near(read(),[64/255,128/255,192/255,128/255]),'normalized DOM channels');
            gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL,true);
            gl.texSubImage2D(gl.TEXTURE_2D,0,0,0,gl.RGBA,type,source);
            error(0,'DOM subupload');
            assert(near(read(),[64/255*128/255,128/255*128/255,192/255*128/255,128/255]),'precise premultiplication');
        }}
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_hdr_renderbuffers_are_implicitly_enabled_and_clear_is_unclamped() {
    check(&format!(
        r#"{HELPERS}
        gl.renderbufferStorage(gl.RENDERBUFFER,0x8814,1,1);error(gl.INVALID_ENUM,'float storage before extension');
        assert(gl.getExtension('OES_texture_float'),'float enables renderability');
        const framebuffer=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);
        gl.bindRenderbuffer(gl.RENDERBUFFER,gl.createRenderbuffer());
        for (const format of [0x8814,0x881a]) {{
            if(format===0x881a)assert(gl.getExtension('OES_texture_half_float'),'half implicit renderability');
            gl.renderbufferStorage(gl.RENDERBUFFER,format,2,2);error(0,'renderbuffer storage');
            gl.framebufferRenderbuffer(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.RENDERBUFFER,gl.getParameter(gl.RENDERBUFFER_BINDING));
            assert(gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE,'float renderbuffer complete');
            assert(gl.getRenderbufferParameter(gl.RENDERBUFFER,gl.RENDERBUFFER_INTERNAL_FORMAT)===format,'sized format preserved');
            gl.clearColor(4,-2,.25,1);gl.clear(gl.COLOR_BUFFER_BIT);
            assert(near(read(),[4,-2,.25,1]),'HDR clear values');
            assert(near([...gl.getParameter(gl.COLOR_CLEAR_VALUE)],[4,-2,.25,1]),'clear state not clamped');
        }}
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_float_upload_rejects_wrong_views_and_enforces_expanded_storage_budget() {
    check(&format!(
        r#"{HELPERS}
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,1,1,0,gl.RGBA,gl.FLOAT,new Float32Array(4));
        error(gl.INVALID_ENUM,'float disabled');
        assert(gl.getExtension('OES_texture_float'),'float capability');
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,1,1,0,gl.RGBA,gl.FLOAT,new Uint16Array(4));
        error(gl.INVALID_OPERATION,'wrong typed view');
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,1,1,0,gl.RGBA,gl.FLOAT,new Float32Array(3));
        error(gl.INVALID_OPERATION,'undersized view');
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,2048,2048,0,gl.RGBA,gl.FLOAT,null);
        error(gl.OUT_OF_MEMORY,'expanded storage exceeds remaining context budget');
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,1,1,0,gl.RGBA,gl.FLOAT,new Float32Array([1,0,0,1]));
        error(0,'bounded upload still works after rejection');
        attach();assert(near(read(),[1,0,0,1]),'rejection did not kill renderer');
        document.querySelector('output').textContent='pass';
    "#
    ));
}
