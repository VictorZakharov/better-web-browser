    const webGlBindingTypes = new Map([[0x8894,'WebGLBuffer'], [0x8895,'WebGLBuffer'],
        [0x8b8d,'WebGLProgram'], [0x8ca6,'WebGLFramebuffer'], [0x8ca7,'WebGLRenderbuffer'],
        [0x8069,'WebGLTexture'], [0x8514,'WebGLTexture']]);
    const webGlFloatParameters = new Set([0x8005, 0x0c22, 0x0b70, 0x846d, 0x846e]);
    const webGlIntParameters = new Set([0x0ba2, 0x0c10, 0x0d3a]);
    webGlMethod('getParameter', function(pname) {
        pname = webGlUnsigned(pname);
        const state = webGlState(this);
        if (pname === 0x9240) return state.unpackFlip;
        if (pname === 0x9241) return state.unpackPremultiply;
        if (pname === 0x9243) return state.unpackColorSpace;
        const result = webGlCall(this, 'getParameter', [pname]);
        if (webGlBindingTypes.has(pname)) return webGlObject(this, webGlBindingTypes.get(pname), result);
        if (result === null) return null;
        if (webGlFloatParameters.has(pname)) return new Float32Array(result);
        if (webGlIntParameters.has(pname)) return new Int32Array(result);
        if (pname === 0x86a3) return new Uint32Array(result);
        return result;
    });
    for (const name of ['getActiveUniform', 'getActiveAttrib', 'getShaderPrecisionFormat']) webGlMethod(name, function(object, index) {
        const args = name === 'getShaderPrecisionFormat' ? [webGlUnsigned(object), webGlUnsigned(index)] :
            [webGlHandle(this, object, 'WebGLProgram'), webGlUnsigned(index)];
        if (args[0] < 0) return null;
        const result = webGlCall(this, name, args);
        if (!result) return null;
        const value = new webGlObjectClasses[name === 'getShaderPrecisionFormat' ?
            'WebGLShaderPrecisionFormat' : 'WebGLActiveInfo'](webGlToken);
        for (const [key, entry] of Object.entries(result)) Object.defineProperty(value, key, {enumerable:true, value:entry});
        return value;
    });
    webGlMethod('getVertexAttrib', function(index, pname) {
        const result = webGlCall(this, 'getVertexAttrib', [webGlUnsigned(index), webGlUnsigned(pname)]);
        if (pname === 0x889f) return webGlObject(this, 'WebGLBuffer', result);
        return pname === 0x8626 && result ? new Float32Array(result) : result;
    });
    webGlMethod('getUniform', function(program, location) {
        const p = webGlHandle(this, program, 'WebGLProgram'), l = webGlHandle(this, location, 'WebGLUniformLocation');
        if (p < 0 || l < 0) return null;
        const record = webGlObjects.get(location);
        if (record.program !== program) { webGlError(this, 0x0502); return null; }
        const count = this.getProgramParameter(program, 0x8b86);
        let kind = null;
        for (let index = 0; index < count; index++) {
            const active = this.getActiveUniform(program, index);
            if (active && active.name.replace(/\[0\]$/, '') === record.name.replace(/\[\d+\]$/, '')) { kind = active.type; break; }
        }
        if (kind === null) { webGlError(this, 0x0502); return null; }
        const result = webGlCall(this, 'getUniform', [p, l, kind]);
        if (!result) return null;
        const bool = [0x8b56, 0x8b57, 0x8b58, 0x8b59].includes(kind);
        if (result.length === 1) return bool ? Boolean(result[0]) : result[0];
        if (bool) return result.map(Boolean);
        return [0x8b53, 0x8b54, 0x8b55].includes(kind) ? new Int32Array(result) : new Float32Array(result);
    });
    webGlMethod('framebufferTexture2D', function(target, attachment, textarget, texture, level) {
        const id = webGlHandle(this, texture, 'WebGLTexture', true);
        if (id >= 0) webGlCall(this, 'framebufferTexture2D', [webGlUnsigned(target), webGlUnsigned(attachment), webGlUnsigned(textarget), id, webGlInteger(level)]);
    });
    webGlMethod('framebufferRenderbuffer', function(target, attachment, renderbufferTarget, renderbuffer) {
        const id = webGlHandle(this, renderbuffer, 'WebGLRenderbuffer', true);
        if (id >= 0) webGlCall(this, 'framebufferRenderbuffer', [webGlUnsigned(target), webGlUnsigned(attachment), webGlUnsigned(renderbufferTarget), id]);
    });
    webGlMethod('getFramebufferAttachmentParameter', function(target, attachment, pname) {
        const result = webGlCall(this, 'getFramebufferAttachmentParameter', [webGlUnsigned(target), webGlUnsigned(attachment), webGlUnsigned(pname)]);
        if (pname !== 0x8cd1 || !result) return result;
        return webGlState(this).objects.get(result) ?? null;
    });
    webGlMethod('readPixels', function(x, y, width, height, format, type, pixels) {
        webGlState(this);
        if (pixels === null) { webGlError(this, 0x0501); return; }
        if (!(pixels instanceof Uint8Array) && !(pixels instanceof Uint8ClampedArray)) {
            webGlError(this, 0x0502); return;
        }
        const bytes = webGlCall(this, 'readPixels', [webGlInteger(x), webGlInteger(y),
            webGlInteger(width), webGlInteger(height), webGlUnsigned(format), webGlUnsigned(type), pixels.byteLength], [], '', webGlBytes(pixels));
        if (bytes) pixels.set(bytes);
    });
