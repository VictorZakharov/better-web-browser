    // §3.7.16 distinguishes ordinary sequences from typed block-index arrays.
    webGl2Method('getFragDataLocation',2,'-s',[[0,'WebGLProgram',false]],function(program,name) {
        const id = webGl2Handle(this,program,'WebGLProgram');
        return id < 0 ? -1 : webGlCall(this,'getFragDataLocation',[id],[],name) ?? -1;
    },-1);
    webGl2Method('getUniformIndices',2,'-S',[[0,'WebGLProgram',false]],function(program,names) {
        const id = webGl2Handle(this,program,'WebGLProgram');
        return id < 0 ? null : webGlCall(this,'getUniformIndices',[id],[],webGlWireList(names));
    },null);
    webGl2Method('getActiveUniforms',3,'-Uu',[[0,'WebGLProgram',false]],function(program,indices,pname) {
        const id = webGl2Handle(this,program,'WebGLProgram');
        return id < 0 ? null : webGlCall(this,'getActiveUniforms',[id,pname],[],webGlWireList(indices));
    },null);
    webGl2Method('getUniformBlockIndex',2,'-s',[[0,'WebGLProgram',false]],function(program,name) {
        const id = webGl2Handle(this,program,'WebGLProgram');
        return id < 0 ? 0xffffffff : webGlCall(this,'getUniformBlockIndex',[id],[],name) ?? 0xffffffff;
    },0xffffffff);
    for (const [name,signature] of [['getActiveUniformBlockParameter','-uu'],
        ['getActiveUniformBlockName','-u'], ['uniformBlockBinding','-uu']]) {
        webGl2Method(name,signature.length,signature,[[0,'WebGLProgram',false]],function(program,...args) {
            const id = webGl2Handle(this,program,'WebGLProgram');
            if (id < 0) return name === 'uniformBlockBinding' ? undefined : null;
            if (name === 'uniformBlockBinding') { webGl2Invoke(this,name,[id,...args.slice(0,2)]); return; }
            const result = webGlCall(this,name,[id,...args.slice(0,signature.length-1)]);
            return name === 'getActiveUniformBlockParameter' && args[1] === 0x8a43 && result !== null
                ? new Uint32Array(result) : result;
        },name === 'uniformBlockBinding' ? undefined : null);
    }
