    // Only browser handles cross IPC. ANGLE's pointer-valued fences stay on
    // their native owner; completion becomes observable between owner tasks.
    webGl2Method('fenceSync',2,'uu',[],function(condition,flags) {
        return webGlObject(this,'WebGLSync',webGlCall(this,'fenceSync',[condition,flags]));
    },null);
    webGl2Method('isSync',1,'-',[[0,'WebGLSync',true]],function(sync) {
        const record = webGlObjects.get(sync), state = webGl2State(this);
        return Boolean(record && record.context === this && record.epoch === state.epoch &&
            !record.deleted && webGlCall(this,'isSync',[record.id]));
    },false);
    webGl2Method('deleteSync',1,'-',[[0,'WebGLSync',true]],function(sync) {
        webGl2Delete(this,sync,'Sync');
    });
    webGl2Method('clientWaitSync',3,'-ua',[[0,'WebGLSync',false]],function(sync,flags,timeout) {
        const id = webGl2Handle(this,sync,'WebGLSync');
        if (id < 0) return 0x911d;
        // MAX_CLIENT_WAIT_TIMEOUT_WEBGL is truthfully zero. Reject before the
        // integer IPC transport so a rounded 64-bit Number cannot lose context.
        if ((flags & ~1) !== 0 || timeout !== 0) {
            webGlError(this,0x0502); return 0x911d;
        }
        return webGlCall(this,'clientWaitSync',[id,flags,timeout]) ?? 0x911d;
    },0x911d);
    webGl2Method('waitSync',3,'-ul',[[0,'WebGLSync',false]],function(sync,flags,timeout) {
        const id = webGl2Handle(this,sync,'WebGLSync');
        if (id < 0) return;
        // Web IDL GLint64 preserves TIMEOUT_IGNORED (-1), unlike GLuint64.
        if (flags !== 0 || timeout !== -1) { webGlError(this,0x0501); return; }
        webGl2Invoke(this,'waitSync',[id,flags,timeout]);
    });
    webGl2Method('getSyncParameter',2,'-u',[[0,'WebGLSync',false]],function(sync,pname) {
        const id = webGl2Handle(this,sync,'WebGLSync');
        return id < 0 ? null : webGlCall(this,'getSyncParameter',[id,pname]);
    },null);
