    const webGl2BindingTypes = new Map(webGlBindingTypes);
    for (const [pname,type] of [[0x85b5,'WebGLVertexArrayObject'], [0x8919,'WebGLSampler'],
        [0x8e25,'WebGLTransformFeedback'], [0x8caa,'WebGLFramebuffer'],
        [0x806a,'WebGLTexture'], [0x8c1d,'WebGLTexture'], [0x8a28,'WebGLBuffer'],
        [0x8c8f,'WebGLBuffer'], [0x8f36,'WebGLBuffer'], [0x8f37,'WebGLBuffer'],
        [0x88ed,'WebGLBuffer'], [0x88ef,'WebGLBuffer']]) webGl2BindingTypes.set(pname,type);
    webGl2Method('getParameter',1,'u',[],function(pname) {
        if ([0x9240,0x9241,0x9243].includes(pname))
            return Reflect.apply(webGlPrototype.getParameter,this,[pname]);
        const result = webGlCall(this,'getParameter',[pname]);
        if (webGl2BindingTypes.has(pname)) return webGlObject(this,webGl2BindingTypes.get(pname),result);
        if (result === null) return null;
        if (webGlFloatParameters.has(pname)) return new Float32Array(result);
        if (webGlIntParameters.has(pname)) return new Int32Array(result);
        if (pname === 0x86a3) return new Uint32Array(result);
        return result;
    },null);
    webGl2Method('getVertexAttrib',2,'uu',[],function(index,pname) {
        const result = webGlCall(this,'getVertexAttrib',[index,pname]);
        if (pname === 0x889f) return webGlObject(this,'WebGLBuffer',result);
        if (pname !== 0x8626 || result === null) return result;
        // Integer current values retain their signed/unsigned native domain.
        if (result.kind === 'int') return new Int32Array(result.values);
        if (result.kind === 'uint') return new Uint32Array(result.values);
        return new Float32Array(result.values ?? result);
    },null);
    webGl2Method('getUniform',2,'--',[[0,'WebGLProgram',false],[1,'WebGLUniformLocation',false]],function(program,location) {
        const p = webGl2Handle(this,program,'WebGLProgram'), l = webGl2Handle(this,location,'WebGLUniformLocation');
        if (p < 0 || l < 0) return null;
        if (webGlObjects.get(location).program !== program) { webGlError(this,0x0502); return null; }
        const result = webGlCall(this,'getUniform',[p,l]);
        if (!result) return null;
        const {kind,values} = result;
        if ([0x8b56,0x8b57,0x8b58,0x8b59].includes(kind))
            return values.length === 1 ? Boolean(values[0]) : values.map(Boolean);
        if (values.length === 1) return values[0];
        if ([0x1405,0x8dc6,0x8dc7,0x8dc8].includes(kind)) return new Uint32Array(values);
        if ([0x1404,0x8b53,0x8b54,0x8b55].includes(kind)) return new Int32Array(values);
        return new Float32Array(values);
    },null);
