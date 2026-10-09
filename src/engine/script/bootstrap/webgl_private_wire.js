    // Closed native command records contain only owned primitive lists. Page
    // JSON/toJSON hooks must not see driver IDs or fabricate native replies.
    // This is not a replacement for observable Web IDL argument conversion.
    const webGlWireStringify = JSON.stringify, webGlWireParseJson = JSON.parse;
    const webGlWireCreate = Object.create, webGlWirePrototype = Object.setPrototypeOf;
    const webGlWireKeys = Object.keys, webGlWireOwn = Object.hasOwn;
    const webGlWireFinite = Number.isFinite, webGlWireNaN = Number.isNaN;
    const webGlWireInteger = Number.isSafeInteger, webGlWireSame = Object.is;
    const webGlWireIncludes = Function.call.bind(String.prototype.includes);
    const webGlWireBytes = Function.call.bind(Function.prototype[Symbol.hasInstance], Uint8Array);
    const webGlWireIsView = ArrayBuffer.isView;
    const webGlWireTypedPrototype = Object.getPrototypeOf(Float32Array.prototype);
    const webGlWireTypedName = Function.call.bind(
        Object.getOwnPropertyDescriptor(webGlWireTypedPrototype,Symbol.toStringTag).get);
    const webGlWireTypedLength = Function.call.bind(
        Object.getOwnPropertyDescriptor(webGlWireTypedPrototype,'length').get);
    const webGlWireFloat32 = values => webGlWireIsView(values) && webGlWireTypedName(values)==='Float32Array';
    // This subset has no string payload, binary upload or observable reply.
    // Native code uses the same bounded queue and validates each GL operation.
    const webGlWireNumericOperations = new Set(('bindBuffer bindBufferBase bindBufferRange bindTexture activeTexture '+
        'texParameteri texParameterf bindFramebuffer bindRenderbuffer framebufferTexture2D framebufferTextureLayer '+
        'framebufferRenderbuffer bindVertexArray bindVertexArrayOES vertexAttribPointer vertexAttribIPointer '+
        'vertexAttribDivisor vertexAttribDivisorANGLE enableVertexAttribArray disableVertexAttribArray enable disable '+
        'viewport scissor clearColor clearDepth clearStencil clear colorMask depthMask depthFunc depthRange blendColor '+
        'blendFunc blendFuncSeparate blendEquation blendEquationSeparate stencilMask stencilMaskSeparate stencilFunc '+
        'stencilFuncSeparate stencilOp stencilOpSeparate cullFace frontFace polygonOffset sampleCoverage lineWidth '+
        'pixelStorei useProgram uniform1f uniform2f uniform3f uniform4f uniform1i uniform2i uniform3i uniform4i '+
        'uniform1fv uniform2fv uniform3fv uniform4fv uniform1iv uniform2iv uniform3iv uniform4iv '+
        'uniformMatrix2fv uniformMatrix3fv uniformMatrix4fv uniform1ui uniform2ui uniform3ui uniform4ui '+
        'uniform1uiv uniform2uiv uniform3uiv uniform4uiv uniformMatrix2x3fv uniformMatrix2x4fv '+
        'uniformMatrix3x2fv uniformMatrix3x4fv uniformMatrix4x2fv uniformMatrix4x3fv '+
        'drawArrays drawElements drawArraysInstanced drawElementsInstanced drawArraysInstancedANGLE '+
        'drawElementsInstancedANGLE drawRangeElements drawBuffers drawBuffersWEBGL readBuffer beginQuery endQuery '+
        'beginTransformFeedback endTransformFeedback pauseTransformFeedback resumeTransformFeedback bindTransformFeedback '+
        'deleteBuffer deleteTexture deleteFramebuffer deleteRenderbuffer deleteShader deleteProgram deleteVertexArray '+
        'deleteVertexArrayOES bridgeError').split(' '));
    const webGlWireHasNumeric = Function.call.bind(Set.prototype.has, webGlWireNumericOperations);
    const webGlWireNumericCandidate = (op, integers, floats, text, bytes) => {
        const typed=webGlWireFloat32(floats), length=typed?webGlWireTypedLength(floats):floats.length;
        if (text !== '' || bytes !== undefined || integers.length+length > 64 || !webGlWireHasNumeric(op)) return false;
        for (let i=0;i<integers.length;i++) if (!webGlWireInteger(integers[i])) return false;
        if (!typed) for (let i=0;i<length;i++) if (typeof floats[i] !== 'number') return false;
        return true;
    };
    const webGlWirePrimitive = value => {
        const type = typeof value;
        if (value !== null && type !== 'undefined' && type !== 'boolean' && type !== 'number' && type !== 'string')
            throw new TypeError('WebGL private wire requires converted primitive values');
        return value;
    };
    const webGlWireArray = values => {
        const result = [];
        webGlWirePrototype(result, null);
        for (let i = 0; i < values.length; i++) result[i] = webGlWirePrimitive(values[i]);
        return result;
    };
    const webGlWireList = values => webGlWireStringify(webGlWireArray(values));
    const webGlWireOptions = values => {
        const record = webGlWireCreate(null), keys = webGlWireKeys(values);
        for (let i = 0; i < keys.length; i++) record[keys[i]] = webGlWirePrimitive(values[keys[i]]);
        return webGlWireStringify(record);
    };
    const webGlWireCommand = (op, integers = [], floats = [], text = '') => {
        const record = webGlWireCreate(null), encoded = [];
        webGlWirePrototype(encoded, null);
        for (let i = 0; i < integers.length; i++)
            if (!webGlWireInteger(integers[i])) return null;
        for (let i = 0; i < floats.length; i++) {
            const value = floats[i];
            if (typeof value !== 'number') return null;
            encoded[i] = webGlWireSame(value, -0) ? '-0' : webGlWireFinite(value) ? value :
                webGlWireNaN(value) ? 'nan' : value > 0 ? 'inf' : '-inf';
        }
        record.op = webGlWirePrimitive(op); record.i = webGlWireArray(integers); record.f = encoded; record.text = webGlWirePrimitive(text);
        return webGlWireStringify(record);
    };
    const webGlWireParse = raw => {
        // Ordinary replies avoid the recursive walk. Only genuine native float
        // sentinel records need a reviver, never an inherited author property.
        return webGlWireParseJson(raw, webGlWireIncludes(raw, '"webglFloat"') ? (key, entry) => {
            if (entry && typeof entry === 'object' && webGlWireKeys(entry).length === 1 && webGlWireOwn(entry, 'webglFloat'))
                return entry.webglFloat === '-0' ? -0 : entry.webglFloat === 'nan' ? NaN : entry.webglFloat === 'inf' ? Infinity : -Infinity;
            return entry;
        } : undefined);
    };
    const webGlWireLost = value => value !== null && typeof value === 'object' &&
        webGlWireOwn(value, 'lost') && value.lost;
