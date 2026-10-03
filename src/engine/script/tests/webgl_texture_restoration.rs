use super::webgl_lifecycle::run;

#[test]
fn webgl_restoration_resets_texture_capability_admission_and_binary_readback() {
    run(r#"
        const canvas=document.querySelector('canvas');
        const gl=canvas.getContext('webgl',{preserveDrawingBuffer:true});
        const assert=(v,s)=>{if(!v)throw Error(s)};
        const error=(expected,s)=>assert(gl.getError()===expected,s);
        const names=['OES_texture_float','OES_texture_half_float','OES_texture_float_linear',
            'OES_texture_half_float_linear','WEBGL_color_buffer_float',
            'EXT_color_buffer_half_float','WEBGL_depth_texture','EXT_sRGB'];
        const old=names.map(name=>gl.getExtension(name));
        assert(old.every(Boolean),'original texture capabilities');
        const texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,texture);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,1,1,0,gl.RGBA,gl.FLOAT,new Float32Array([2,-1,.5,1]));
        const fbo=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,fbo);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        assert(gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE,'original HDR target');
        const before=new Float32Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.FLOAT,before);
        assert([...before].join()==='2,-1,0.5,1','original binary float read');error(0,'original read');
        const loss=gl.getExtension('WEBGL_lose_context');
        canvas.addEventListener('webglcontextlost',event=>{
            event.preventDefault();
            const bytes=new Uint8Array([7,8,9,10]);
            gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,bytes);
            assert([...bytes].join()==='7,8,9,10','loss leaves binary destination unchanged');
            assert(gl.getError()===gl.CONTEXT_LOST_WEBGL,'loss error');
            setTimeout(()=>loss.restoreContext(),0);
        });
        canvas.addEventListener('webglcontextrestored',()=>{
            assert(!gl.isTexture(texture)&&!gl.isFramebuffer(fbo),'old resource epoch retired');
            gl.bindFramebuffer(gl.FRAMEBUFFER,fbo);error(gl.INVALID_OPERATION,'stale framebuffer');
            const replacement=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,replacement);
            for(const [format,type] of [[gl.RGBA,gl.FLOAT],[gl.RGBA,0x8d61],
                [gl.DEPTH_COMPONENT,gl.UNSIGNED_SHORT],[0x8c42,gl.UNSIGNED_BYTE]]){
                gl.texImage2D(gl.TEXTURE_2D,0,format,1,1,0,format,type,null);
                error(gl.INVALID_ENUM,'restored native capabilities are not author enabled');
            }
            const current=names.map(name=>gl.getExtension(name));
            assert(current.every((value,index)=>value&&value!==old[index]),'fresh extension identities');
            gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
            const after=new Uint8Array([7,8,9,10]);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,after);
            assert([...after].join()==='0,255,0,255','restored owner returns fresh native bytes');
            error(0,'restored default read');canvas.dataset.result='pass';
        });
        loss.loseContext();
    "#);
}
