    // WebGL typedefs use Web IDL conversion, not Math.trunc for every slot.
    // Convert in argument order before context-loss handling or native validation.
    // https://webidl.spec.whatwg.org/#es-long
    const webGlScalarArguments = new Map();
    const webGlScalarSignature = (names, signature) => {
        for (const name of names.split(' ')) webGlScalarArguments.set(name, signature);
    };
    // u = unsigned long, i = long, l = long long, f = unrestricted float,
    // b = boolean, s = DOMString, - = interface/buffer/sequence conversion.
    webGlScalarSignature('activeTexture clear enable disable depthFunc blendEquation frontFace cullFace stencilMask generateMipmap isEnabled checkFramebufferStatus getParameter enableVertexAttribArray disableVertexAttribArray createShader', 'u');
    webGlScalarSignature('clearStencil', 'i');
    webGlScalarSignature('blendFunc blendEquationSeparate stencilMaskSeparate getBufferParameter getRenderbufferParameter getTexParameter getVertexAttrib getVertexAttribOffset getShaderPrecisionFormat hint', 'uu');
    webGlScalarSignature('blendFuncSeparate', 'uuuu');
    webGlScalarSignature('stencilOp', 'uuu');
    webGlScalarSignature('stencilOpSeparate', 'uuuu');
    webGlScalarSignature('stencilFunc', 'uiu');
    webGlScalarSignature('stencilFuncSeparate', 'uuiu');
    webGlScalarSignature('viewport scissor', 'iiii');
    webGlScalarSignature('renderbufferStorage', 'uuii');
    webGlScalarSignature('drawArrays', 'uii');
    webGlScalarSignature('drawElements', 'uiul');
    webGlScalarSignature('copyTexImage2D', 'uiuiiiii');
    webGlScalarSignature('copyTexSubImage2D', 'uiiiiiii');
    webGlScalarSignature('texParameteri', 'uui');
    webGlScalarSignature('texParameterf', 'uuf');
    webGlScalarSignature('clearColor blendColor', 'ffff');
    webGlScalarSignature('clearDepth lineWidth', 'f');
    webGlScalarSignature('depthRange polygonOffset', 'ff');
    webGlScalarSignature('colorMask', 'bbbb');
    webGlScalarSignature('depthMask', 'b');
    webGlScalarSignature('sampleCoverage', 'fb');
    webGlScalarSignature('bindBuffer bindTexture bindFramebuffer bindRenderbuffer', 'u-');
    webGlScalarSignature('getShaderParameter getProgramParameter getActiveUniform getActiveAttrib', '-u');
    webGlScalarSignature('shaderSource getAttribLocation getUniformLocation', '-s');
    webGlScalarSignature('bindAttribLocation', '-us');
    webGlScalarSignature('framebufferTexture2D', 'uuu-i');
    webGlScalarSignature('framebufferRenderbuffer', 'uuu-');
    webGlScalarSignature('getFramebufferAttachmentParameter', 'uuu');
    webGlScalarSignature('vertexAttribPointer', 'uiubil');
    webGlScalarSignature('bufferSubData', 'ul-');
    webGlScalarSignature('bufferData', 'u-u');
    webGlScalarSignature('pixelStorei', 'ui');
    webGlScalarSignature('readPixels', 'iiiiuu-');
    webGlScalarSignature('compressedTexImage2D', 'uiuiii-');
    webGlScalarSignature('compressedTexSubImage2D', 'uiiiiiu-');
    for (let count = 1; count <= 4; count++) {
        for (const kind of ['f', 'i']) {
            webGlScalarArguments.set('uniform' + count + kind, '-' + kind.repeat(count));
            webGlScalarArguments.set('uniform' + count + kind + 'v', '-' + (kind === 'f' ? 'F' : 'I'));
        }
        webGlScalarArguments.set('vertexAttrib' + count + 'f', 'u' + 'f'.repeat(count));
        webGlScalarArguments.set('vertexAttrib' + count + 'fv', 'uF');
    }
    for (let count = 2; count <= 4; count++) webGlScalarArguments.set('uniformMatrix' + count + 'fv', '-bF');

    const webGlNumber = value => +value; // ToNumber rejects BigInt, including boxed BigInt.
    const webGlLongLong = value => {
        const number = webGlNumber(value);
        if (!Number.isFinite(number) || number === 0) return 0;
        // BigInt is used internally only to retain the 64-bit modulo operation.
        return Number(BigInt.asIntN(64, BigInt(Math.trunc(number))));
    };
    const webGlScalar = (kind, value) => {
        if (kind === 'F' || kind === 'I') {
            const method = value === null || value === undefined ? undefined : value[Symbol.iterator];
            if (typeof method !== 'function')
                throw new TypeError('WebGL numeric list must be iterable');
            const iterator = Reflect.apply(method, value, []);
            const values = [];
            for (const entry of {[Symbol.iterator]:() => iterator}) {
                if (values.length >= 8192) throw new RangeError('WebGL numeric list exceeds the command budget');
                values.push(webGlScalar(kind === 'F' ? 'f' : 'i', entry));
            }
            return values;
        }
        if (kind === 'u') return webGlNumber(value) >>> 0;
        if (kind === 'i') return webGlNumber(value) | 0;
        if (kind === 'l') return webGlLongLong(value);
        if (kind === 'f') return Math.fround(webGlNumber(value));
        if (kind === 'b') return Boolean(value);
        if (kind === 's') {
            if (typeof value === 'symbol') throw new TypeError('Cannot convert Symbol to DOMString');
            return String(value);
        }
        return value;
    };
    const webGlConvertArguments = (name, args) => {
        let signature = webGlScalarArguments.get(name) ?? '';
        if (name === 'texImage2D') signature = args.length >= 9 ? 'uiiiiiuu-' : 'uiiuu-';
        if (name === 'texSubImage2D') signature = args.length >= 9 ? 'uiiiiiuu-' : 'uiiiuu-';
        const interfaces = webGlInterfaceArguments.get(name) ?? [];
        const count = Math.max(signature.length, ...interfaces.map(entry => entry[0] + 1));
        for (let index = 0; index < count; index++) {
            const entry = interfaces.find(entry => entry[0] === index);
            if (entry) webGlConvertInterface(args, ...entry);
            if (signature[index] && signature[index] !== '-') args[index] = webGlScalar(signature[index], args[index]);
            if (name === 'bufferData' && index === 1 && args[index] !== null &&
                !(args[index] instanceof ArrayBuffer) && !ArrayBuffer.isView(args[index])) args[index] = webGlLongLong(args[index]);
            if ((name === 'readPixels' && index === 6 || name.startsWith('compressedTex') && index === signature.length - 1 ||
                (name === 'texImage2D' || name === 'texSubImage2D') && signature.length === 9 && index === 8) &&
                args[index] !== null && !ArrayBuffer.isView(args[index])) throw new TypeError('Expected ArrayBufferView');
            if (name === 'bufferSubData' && index === 2 && !(args[index] instanceof ArrayBuffer) && !ArrayBuffer.isView(args[index]))
                throw new TypeError('Expected BufferSource');
        }
        // Web IDL ignores additional arguments. Texture overload selection uses
        // effective argument count first; its implementation receives that shape.
        if (name === 'texImage2D' || name === 'texSubImage2D') args.length = signature.length;
        else args.length = Math.min(args.length, webGlArities[name]);
    };
