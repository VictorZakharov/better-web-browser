    const webGlPrototype = WebGLRenderingContext.prototype;
    const webGlArities = {
        createBuffer:0, createShader:1, createProgram:0, createTexture:0, createFramebuffer:0, createRenderbuffer:0,
        deleteBuffer:1, deleteShader:1, deleteProgram:1, deleteTexture:1, deleteFramebuffer:1, deleteRenderbuffer:1,
        isBuffer:1, isShader:1, isProgram:1, isTexture:1, isFramebuffer:1, isRenderbuffer:1,
        bindBuffer:2, bindTexture:2, bindFramebuffer:2, bindRenderbuffer:2,
        compileShader:1, linkProgram:1, validateProgram:1, useProgram:1, shaderSource:2,
        getShaderInfoLog:1, getProgramInfoLog:1, getShaderSource:1, getAttachedShaders:1,
        attachShader:2, detachShader:2, getShaderParameter:2, getProgramParameter:2,
        getAttribLocation:2, bindAttribLocation:3, getUniformLocation:2,
        uniform1f:2, uniform2f:3, uniform3f:4, uniform4f:5, uniform1i:2, uniform2i:3, uniform3i:4, uniform4i:5,
        uniform1fv:2, uniform2fv:2, uniform3fv:2, uniform4fv:2, uniform1iv:2, uniform2iv:2, uniform3iv:2, uniform4iv:2,
        uniformMatrix2fv:3, uniformMatrix3fv:3, uniformMatrix4fv:3, bufferData:3, bufferSubData:3,
        vertexAttribPointer:6, vertexAttrib1f:2, vertexAttrib2f:3, vertexAttrib3f:4, vertexAttrib4f:5,
        vertexAttrib1fv:2, vertexAttrib2fv:2, vertexAttrib3fv:2, vertexAttrib4fv:2,
        activeTexture:1, clear:1, clearStencil:1, enable:1, disable:1,
        enableVertexAttribArray:1, disableVertexAttribArray:1, drawArrays:3, drawElements:4,
        viewport:4, scissor:4, depthFunc:1, blendFunc:2, blendFuncSeparate:4, blendEquation:1,
        blendEquationSeparate:2, frontFace:1, cullFace:1, stencilFunc:3, stencilFuncSeparate:4,
        stencilMask:1, stencilMaskSeparate:2, stencilOp:3, stencilOpSeparate:4,
        renderbufferStorage:4, framebufferTexture2D:5, framebufferRenderbuffer:4,
        checkFramebufferStatus:1, getBufferParameter:2, getRenderbufferParameter:2,
        getTexParameter:2, texParameteri:3, texParameterf:3, generateMipmap:1, isEnabled:1,
        getVertexAttribOffset:2, hint:2, copyTexImage2D:8, copyTexSubImage2D:8,
        clearColor:4, clearDepth:1, blendColor:4, depthRange:2, polygonOffset:2, lineWidth:1,
        colorMask:4, depthMask:1, sampleCoverage:2, flush:0, finish:0,
        getParameter:1, getActiveUniform:2, getActiveAttrib:2, getShaderPrecisionFormat:2,
        getVertexAttrib:2, getUniform:2, getFramebufferAttachmentParameter:3, readPixels:7,
        pixelStorei:2, texImage2D:6, texSubImage2D:7, compressedTexImage2D:7, compressedTexSubImage2D:8
    };
    const webGlResultMethods = new Set(['getAttribLocation','getUniformLocation',
        'checkFramebufferStatus','isEnabled','getVertexAttribOffset']);
    const webGlMethod = (name, implementation) => {
        const arity = webGlArities[name];
        if (arity === undefined) throw new Error('Missing WebGL IDL signature: ' + name);
        const argumentPlan = webGlArgumentPlan(name, arity);
        const method = function(...args) {
            webGlState(this);
            if (args.length < arity) throw new TypeError(name + ' requires at least ' + arity + ' arguments');
            const valid=webGlConvertArguments(argumentPlan, args);
            if (webGlState(this).lost) {
                // KHR_parallel_shader_compile requires true after loss so a
                // retained extension's polling loop cannot wait forever.
                if ((name === 'getProgramParameter' || name === 'getShaderParameter') && args[1] === 0x91b1)
                    return true;
                if (name === 'getAttribLocation') return -1;
                if (name === 'checkFramebufferStatus') return 0x8cdd;
                if (name === 'getVertexAttribOffset') return 0;
                if (name.startsWith('is')) return false;
                if (/^(create|get)/.test(name)) return null;
                return undefined;
            }
            if (!valid) {webGlError(this,0x0501);return undefined;}
            const result = Reflect.apply(implementation, this, args);
            return /^(create|get|is)/.test(name) || webGlResultMethods.has(name) ? result : undefined;
        };
        Object.defineProperties(method, {name:{value:name}, length:{value:arity}});
        Object.defineProperty(webGlPrototype, name, {enumerable:true, configurable:true, writable:true, value:method});
    };
    for (const type of ['Buffer', 'Shader', 'Program', 'Texture', 'Framebuffer', 'Renderbuffer']) {
        webGlMethod('create' + type, function(...args) {
            return webGlObject(this, 'WebGL' + type, webGlCall(this, 'create' + type,
                type === 'Shader' ? [webGlUnsigned(args[0])] : []));
        });
        webGlMethod('delete' + type, function(value) {
            if (value === null) { webGlState(this); return; }
            const object = webGlObjects.get(value);
            if (!object || object.type !== 'WebGL' + type) throw new TypeError('Expected WebGL' + type);
            if (object.context !== this || object.epoch !== webGlState(this).epoch) { webGlError(this, 0x0502); return; }
            if (object.deleted) return;
            webGlCall(this, 'delete' + type, [object.id]);
            object.deleted = true;
        });
        webGlMethod('is' + type, function(value) {
            const state = webGlState(this), object = webGlObjects.get(value);
            return !state.lost && Boolean(object && object.context === this &&
                object.epoch === state.epoch &&
                object.type === 'WebGL' + type &&
                webGlCall(this, 'is' + type, [object.id]));
        });
    }
    for (const [name, type] of [['bindBuffer', 'Buffer'], ['bindTexture', 'Texture'],
        ['bindFramebuffer', 'Framebuffer'], ['bindRenderbuffer', 'Renderbuffer']]) {
        webGlMethod(name, function(target, value) {
            const id = webGlHandle(this, value, 'WebGL' + type, true);
            if (id >= 0) webGlCall(this, name, [webGlUnsigned(target), id]);
        });
    }
    for (const name of ['compileShader', 'linkProgram', 'validateProgram', 'useProgram',
        'getShaderInfoLog', 'getProgramInfoLog', 'getShaderSource', 'getAttachedShaders']) {
        const type = name.includes('Shader') && name !== 'getAttachedShaders' ? 'Shader' : 'Program';
        webGlMethod(name, function(value) {
            const id = webGlHandle(this, value, 'WebGL' + type, name === 'useProgram');
            if (id < 0) return null;
            const result = webGlCall(this, name, [id]);
            return name === 'getAttachedShaders' ? result?.map(id => webGlObject(this, 'WebGLShader', id)) ?? null : result;
        });
    }
    for (const name of ['attachShader', 'detachShader']) webGlMethod(name, function(program, shader) {
        const p = webGlHandle(this, program, 'WebGLProgram'), s = webGlHandle(this, shader, 'WebGLShader');
        if (p >= 0 && s >= 0) webGlCall(this, name, [p, s]);
    });
    webGlMethod('shaderSource', function(shader, source) {
        const id = webGlHandle(this, shader, 'WebGLShader');
        if (id >= 0) webGlCall(this, 'shaderSource', [id], [], String(source));
    });
    for (const type of ['Shader', 'Program']) webGlMethod('get' + type + 'Parameter', function(object, pname) {
        const id = webGlHandle(this, object, 'WebGL' + type);
        return id < 0 ? null : webGlCall(this, 'get' + type + 'Parameter', [id, webGlUnsigned(pname)]);
    });
    webGlMethod('getAttribLocation', function(program, name) {
        const id = webGlHandle(this, program, 'WebGLProgram');
        return id < 0 ? -1 : webGlCall(this, 'getAttribLocation', [id], [], String(name)) ?? -1;
    });
    webGlMethod('bindAttribLocation', function(program, index, name) {
        const id = webGlHandle(this, program, 'WebGLProgram');
        if (id >= 0) webGlCall(this, 'bindAttribLocation', [id, webGlUnsigned(index)], [], String(name));
    });
    webGlMethod('getUniformLocation', function(program, name) {
        const id = webGlHandle(this, program, 'WebGLProgram');
        if (id < 0) return null;
        const location = webGlObject(this, 'WebGLUniformLocation', webGlCall(this, 'getUniformLocation', [id], [], String(name)));
        if (location) Object.assign(webGlObjects.get(location), {program, name:String(name)});
        return location;
    });
    for (let count = 1; count <= 4; count++) for (const type of ['f', 'i']) for (const vector of ['', 'v']) {
        const name = 'uniform' + count + type + vector;
        webGlMethod(name, function(location, ...input) {
            const id = webGlHandle(this, location, 'WebGLUniformLocation', true);
            if (id <= 0) return;
            const values = vector ? Array.from(input[0], Number) : input.slice(0, count).map(Number);
            if (type === 'i') webGlCall(this, name, [id, ...values.map(webGlInteger)]);
            else webGlCall(this, name, [id], values);
        });
    }
    for (let count = 2; count <= 4; count++) webGlMethod('uniformMatrix' + count + 'fv', function(location, transpose, data) {
        const id = webGlHandle(this, location, 'WebGLUniformLocation', true);
        if (id > 0) webGlCall(this, 'uniformMatrix' + count + 'fv', [id, Number(Boolean(transpose))], Array.from(data, Number));
    });
    webGlMethod('bufferData', function(target, data, usage) {
        const bytes = typeof data === 'number' ? undefined : data === null ? new Uint8Array() : webGlBytes(data);
        webGlCall(this, 'bufferData', [webGlUnsigned(target), bytes?.byteLength ?? Number(data), webGlUnsigned(usage)], [], '', bytes);
    });
    webGlMethod('bufferSubData', function(target, offset, data) {
        webGlCall(this, 'bufferSubData', [webGlUnsigned(target), Number(offset)], [], '', webGlBytes(data));
    });
    webGlMethod('vertexAttribPointer', function(index, size, type, normalized, stride, offset) {
        webGlCall(this, 'vertexAttribPointer', [webGlUnsigned(index), webGlInteger(size), webGlUnsigned(type), Number(Boolean(normalized)), webGlInteger(stride), Number(offset)]);
    });
    for (let count = 1; count <= 4; count++) for (const vector of ['', 'v']) {
        webGlMethod('vertexAttrib' + count + 'f' + vector, function(index, ...input) {
            const values = vector ? Array.from(input[0], Number).slice(0, count) : input.slice(0, count).map(Number);
            webGlCall(this, 'vertexAttrib' + count + 'f', [webGlUnsigned(index)], values);
        });
    }
    const integerMethods = ['activeTexture', 'clear', 'clearStencil', 'enable', 'disable',
        'enableVertexAttribArray', 'disableVertexAttribArray', 'drawArrays', 'drawElements',
        'viewport', 'scissor', 'depthFunc', 'blendFunc', 'blendFuncSeparate', 'blendEquation',
        'blendEquationSeparate', 'frontFace', 'cullFace', 'stencilFunc', 'stencilFuncSeparate',
        'stencilMask', 'stencilMaskSeparate', 'stencilOp', 'stencilOpSeparate',
        'renderbufferStorage', 'framebufferTexture2D', 'framebufferRenderbuffer',
        'checkFramebufferStatus', 'getBufferParameter', 'getRenderbufferParameter',
        'getTexParameter', 'texParameteri', 'texParameterf', 'generateMipmap', 'isEnabled',
        'getVertexAttribOffset', 'hint', 'copyTexImage2D', 'copyTexSubImage2D'];
    // Framebuffer attachment methods below translate their object-valued arguments.
    for (const name of integerMethods.filter(name => !name.startsWith('framebuffer'))) webGlMethod(name, function(...args) {
        const result = name === 'texParameterf'
            ? webGlCall(this, name, args.slice(0, 2), [args[2]])
            : webGlCall(this, name, args);
        if (['clear', 'drawArrays', 'drawElements'].includes(name)) webGlState(this).dirty = true;
        return result;
    });
    for (const name of ['clearColor', 'clearDepth', 'blendColor', 'depthRange', 'polygonOffset', 'lineWidth'])
        webGlMethod(name, function(...args) { webGlCall(this, name, [], args.map(Number)); });
    for (const name of ['colorMask', 'depthMask']) webGlMethod(name, function(...args) {
        webGlCall(this, name, args.map(value => Number(Boolean(value))));
    });
    webGlMethod('sampleCoverage', function(value, invert) { webGlCall(this, 'sampleCoverage', [Number(Boolean(invert))], [Number(value)]); });
    for (const name of ['flush', 'finish']) webGlMethod(name, function() { webGlCall(this, name); });
    for (const name of ['compressedTexImage2D', 'compressedTexSubImage2D'])
        webGlMethod(name, function(...args) {
            webGlState(this);
            const pixels = webGlBytes(args[args.length - 1]);
            webGlCall(this, name, args.slice(0, name === 'compressedTexImage2D' ? 6 : 7), [], '', pixels);
        });
