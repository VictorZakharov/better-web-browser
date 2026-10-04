    for (const [name, signature] of [
        ['readBuffer','u'], ['renderbufferStorageMultisample','uiuii'],
        ['blitFramebuffer','iiiiiiiiuu'], ['copyTexSubImage3D','uiiiiiiii']]) {
        webGl2Method(name,signature.length,signature,[],function(...args) {
            webGl2Invoke(this,name,args.slice(0,signature.length));
            if (name === 'blitFramebuffer') webGl2State(this).dirty = true;
        });
    }
    webGl2Method('drawBuffers',1,'U',[],function(buffers) {
        webGl2Invoke(this,'drawBuffers',buffers);
    });
    webGl2Method('framebufferTextureLayer',5,'uu-ii',[[2,'WebGLTexture',true]],function(target,attachment,texture,level,layer) {
        const id = webGl2Handle(this,texture,'WebGLTexture',true);
        if (id >= 0) webGl2Invoke(this,'framebufferTextureLayer',[target,attachment,id,level,layer]);
    });
    webGl2Method('getInternalformatParameter',3,'uuu',[],function(target,format,pname) {
        const values = webGlCall(this,'getInternalformatParameter',[target,format,pname]);
        return values === null ? null : new Int32Array(values);
    },null);
    webGl2Method('invalidateFramebuffer',2,'uU',[],function(target,attachments) {
        webGl2Invoke(this,'invalidateFramebuffer',[target,...attachments]);
    });
    webGl2Method('invalidateSubFramebuffer',6,'uUiiii',[],function(target,attachments,x,y,width,height) {
        webGl2Invoke(this,'invalidateSubFramebuffer',[target,x,y,width,height,...attachments]);
    });
    for (const kind of ['f','i','u']) {
        const name = 'clearBuffer'+(kind === 'u' ? 'ui' : kind)+'v';
        webGl2Method(name,3,'ui-a',[[2,webGl2NumericArgument(kind)]],function(buffer,index,source,offset) {
            const count = buffer === 0x1800 ? 4 : 1;
            const values = webGl2NumericSlice(this,source,offset,count);
            if (!values) return;
            if (kind === 'f') webGl2Invoke(this,name,[buffer,index],values);
            else webGl2Invoke(this,name,[buffer,index,...values]);
            webGl2State(this).dirty = true;
        });
    }
    webGl2Method('clearBufferfi',4,'uifi',[],function(buffer,index,depth,stencil) {
        webGl2Invoke(this,'clearBufferfi',[buffer,index,stencil],[depth]);
        webGl2State(this).dirty = true;
    });
