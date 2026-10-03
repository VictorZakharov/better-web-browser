use super::webgl_instancing::check;

const SETUP: &str = r#"
    const gl=document.querySelector('canvas').getContext('webgl',{preserveDrawingBuffer:true});
    const assert=(value,label)=>{if(!value)throw Error(label)};
    const error=(expected,label)=>{const actual=gl.getError();assert(actual===expected,label+' GL '+actual)};
    const framebuffer=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);
    const texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,texture);
    gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,1,1,0,gl.RGBA,gl.UNSIGNED_BYTE,new Uint8Array([255,0,0,255]));
    gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
    const storage=format=>{
        const object=gl.createRenderbuffer();gl.bindRenderbuffer(gl.RENDERBUFFER,object);
        gl.renderbufferStorage(gl.RENDERBUFFER,format,1,1);error(0,'storage');return object;
    };
    const complete=()=>assert(gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE,'framebuffer complete');
    const read=()=>{
        const bytes=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,bytes);
        error(0,'read');return [...bytes].join();
    };
"#;

#[test]
fn webgl_depth_stencil_logical_conflicts_reject_writes_and_recover_after_detachment() {
    check(&format!(
        r#"{SETUP}
        const depth=storage(gl.DEPTH_COMPONENT16), stencil=storage(gl.STENCIL_INDEX8), packed=storage(gl.DEPTH_STENCIL);
        const points=[gl.DEPTH_ATTACHMENT,gl.STENCIL_ATTACHMENT,gl.DEPTH_STENCIL_ATTACHMENT];
        const objects=[depth,stencil,packed];
        for(const [first,second] of [[0,1],[0,2],[1,2]]){{
            for(const point of points)gl.framebufferRenderbuffer(gl.FRAMEBUFFER,point,gl.RENDERBUFFER,null);
            gl.framebufferRenderbuffer(gl.FRAMEBUFFER,points[first],gl.RENDERBUFFER,objects[first]);complete();
            gl.framebufferRenderbuffer(gl.FRAMEBUFFER,points[second],gl.RENDERBUFFER,objects[second]);error(0,'conflicting assignments accepted');
            assert(gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_UNSUPPORTED,'logical conflict must be unsupported');
            assert(gl.getFramebufferAttachmentParameter(gl.FRAMEBUFFER,points[first],gl.FRAMEBUFFER_ATTACHMENT_OBJECT_NAME)===objects[first],'first assignment identity');
            assert(gl.getFramebufferAttachmentParameter(gl.FRAMEBUFFER,points[second],gl.FRAMEBUFFER_ATTACHMENT_OBJECT_NAME)===objects[second],'second assignment identity');
            gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);error(gl.INVALID_FRAMEBUFFER_OPERATION,'conflict rejects clear');
            const untouched=new Uint8Array([7,7,7,7]);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,untouched);
            error(gl.INVALID_FRAMEBUFFER_OPERATION,'conflict rejects read');assert([...untouched].join()==='7,7,7,7','failed read untouched');
            gl.framebufferRenderbuffer(gl.FRAMEBUFFER,points[first],gl.RENDERBUFFER,null);complete();
            assert(read()==='255,0,0,255','failed clear preserved original color');
            gl.clear(gl.COLOR_BUFFER_BIT);assert(read()==='0,255,0,255','recovered target writable');
            gl.clearColor(1,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);error(0,'restore fixture color');
        }}
        document.querySelector('output').textContent='pass';
        "#
    ));
}

#[test]
fn webgl_depth_stencil_attachment_formats_do_not_inherit_gles3_relaxations() {
    check(&format!(
        r#"{SETUP}
        const packed=storage(gl.DEPTH_STENCIL), depth=storage(gl.DEPTH_COMPONENT16), stencil=storage(gl.STENCIL_INDEX8);
        for(const [point,object] of [[gl.DEPTH_ATTACHMENT,packed],[gl.STENCIL_ATTACHMENT,packed],
            [gl.DEPTH_STENCIL_ATTACHMENT,depth],[gl.DEPTH_STENCIL_ATTACHMENT,stencil]]){{
            for(const p of [gl.DEPTH_ATTACHMENT,gl.STENCIL_ATTACHMENT,gl.DEPTH_STENCIL_ATTACHMENT])
                gl.framebufferRenderbuffer(gl.FRAMEBUFFER,p,gl.RENDERBUFFER,null);
            gl.framebufferRenderbuffer(gl.FRAMEBUFFER,point,gl.RENDERBUFFER,object);error(0,'wrong format can be attached');
            assert(gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_UNSUPPORTED,'WebGL1 format restriction');
        }}
        document.querySelector('output').textContent='pass';
        "#
    ));
}

#[test]
fn webgl_deleted_texture_remains_owned_by_an_inactive_framebuffer_until_detached() {
    check(&format!(
        r#"{SETUP}
        complete();gl.bindFramebuffer(gl.FRAMEBUFFER,null);gl.deleteTexture(texture);
        assert(!gl.isTexture(texture),'deleted public texture');
        assert(gl.getParameter(gl.TEXTURE_BINDING_2D)===null,'deleted texture unbound');
        gl.bindTexture(gl.TEXTURE_2D,texture);error(gl.INVALID_OPERATION,'deleted texture cannot be rebound');
        gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);complete();
        assert(gl.getFramebufferAttachmentParameter(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.FRAMEBUFFER_ATTACHMENT_OBJECT_NAME)===texture,'retained attachment identity');
        assert(read()==='255,0,0,255','retained image pixels');
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,null,0);error(0,'retire final reference');
        assert(gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_INCOMPLETE_MISSING_ATTACHMENT,'detached retained image');
        gl.deleteFramebuffer(framebuffer);error(0,'framebuffer cleanup');
        document.querySelector('output').textContent='pass';
        "#
    ));
}

#[test]
fn webgl_deleted_renderbuffer_is_retained_and_current_framebuffer_deletion_detaches() {
    check(&format!(
        r#"{SETUP}
        const depth=storage(gl.DEPTH_COMPONENT16);
        gl.framebufferRenderbuffer(gl.FRAMEBUFFER,gl.DEPTH_ATTACHMENT,gl.RENDERBUFFER,depth);complete();
        gl.bindFramebuffer(gl.FRAMEBUFFER,null);gl.deleteRenderbuffer(depth);
        assert(!gl.isRenderbuffer(depth),'deleted public renderbuffer');
        assert(gl.getParameter(gl.RENDERBUFFER_BINDING)===null,'deleted renderbuffer unbound');
        gl.bindRenderbuffer(gl.RENDERBUFFER,depth);error(gl.INVALID_OPERATION,'deleted renderbuffer cannot be rebound');
        gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);complete();
        assert(gl.getFramebufferAttachmentParameter(gl.FRAMEBUFFER,gl.DEPTH_ATTACHMENT,gl.FRAMEBUFFER_ATTACHMENT_OBJECT_NAME)===depth,'retained depth identity');
        gl.clearDepth(.5);gl.clear(gl.DEPTH_BUFFER_BIT);error(0,'retained depth image writable');
        gl.deleteTexture(texture);error(0,'delete currently attached color image');
        assert(gl.getFramebufferAttachmentParameter(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE)===gl.NONE,'current image detached');
        gl.deleteFramebuffer(framebuffer);error(0,'retire retained depth on framebuffer deletion');
        document.querySelector('output').textContent='pass';
        "#
    ));
}
