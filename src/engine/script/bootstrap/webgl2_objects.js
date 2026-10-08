    // Opaque names retain identity, realm ownership and restoration generation.
    // No author-visible property contains the native driver ID or sync pointer.
    const webGl2Delete = (context, value, suffix) => {
        if (value === null) return;
        const record = webGlObjects.get(value), state = webGl2State(context);
        if (record.context !== context || record.epoch !== state.epoch) {
            webGlError(context, 0x0502); return;
        }
        if (record.deleted) return;
        if (webGlCall(context, 'deleteWebGl2Object', [record.id], [], suffix)) record.deleted = true;
    };
    for (const [suffix, type] of [['VertexArray','WebGLVertexArrayObject'],
        ['Sampler','WebGLSampler'], ['Query','WebGLQuery'],
        ['TransformFeedback','WebGLTransformFeedback']]) {
        webGl2Method('create' + suffix, 0, '', [], function() {
            return webGlObject(this, type, webGlCall(this, 'create' + suffix));
        }, null);
        webGl2Method('delete' + suffix, 1, '-', [[0,type,true]], function(value) {
            webGl2Delete(this, value, suffix);
        });
        webGl2Method('is' + suffix, 1, '-', [[0,type,true]], function(value) {
            const record = webGlObjects.get(value), state = webGl2State(this);
            return Boolean(record && record.context === this && record.epoch === state.epoch &&
                !record.deleted && webGlCall(this, 'is' + suffix, [record.id]));
        }, false);
    }
    for (const [name, signature, index, type] of [
        ['bindVertexArray','-',0,'WebGLVertexArrayObject'],
        ['bindSampler','u-',1,'WebGLSampler'],
        ['bindTransformFeedback','u-',1,'WebGLTransformFeedback']]) {
        webGl2Method(name, signature.length, signature, [[index,type,true]], function(...args) {
            const id = webGl2Handle(this, args[index], type, true);
            if (id < 0) return;
            args[index] = id;
            webGl2Invoke(this, name, args.slice(0,signature.length));
        });
    }
    for (const [name, signature] of [
        ['vertexAttribDivisor','uu'], ['drawArraysInstanced','uiii'],
        ['drawElementsInstanced','uiuli'], ['drawRangeElements','uuuiul'],
        ['beginTransformFeedback','u'], ['endTransformFeedback',''],
        ['pauseTransformFeedback',''], ['resumeTransformFeedback','']]) {
        webGl2Method(name, signature.length, signature, [], function(...args) {
            webGl2Invoke(this, name, args.slice(0,signature.length));
            if (name.startsWith('draw')) webGl2State(this).dirty = true;
        });
    }
    webGl2Method('transformFeedbackVaryings', 3, '-Su', [[0,'WebGLProgram',false]], function(program, varyings, mode) {
        const id = webGl2Handle(this, program, 'WebGLProgram');
        if (id >= 0) webGl2Invoke(this, 'transformFeedbackVaryings', [id, mode], [], webGlWireList(varyings));
    });
    webGl2Method('getTransformFeedbackVarying', 2, '-u', [[0,'WebGLProgram',false]], function(program, index) {
        const id = webGl2Handle(this, program, 'WebGLProgram');
        if (id < 0) return null;
        const result = webGlCall(this, 'getTransformFeedbackVarying', [id, index]);
        if (!result) return null;
        return webGlReflectionRecord('WebGLActiveInfo', result);
    }, null);
    webGl2Method('beginQuery', 2, 'u-', [[1,'WebGLQuery',false]], function(target, query) {
        const id = webGl2Handle(this, query, 'WebGLQuery');
        if (id >= 0) webGl2Invoke(this, 'beginQuery', [target, id]);
    });
    webGl2Method('endQuery', 1, 'u', [], function(target) { webGl2Invoke(this, 'endQuery', [target]); });
    webGl2Method('getQuery', 2, 'uu', [], function(target, pname) {
        const result = webGlCall(this, 'getQuery', [target,pname]);
        return pname === 0x8865 ? webGlObject(this, 'WebGLQuery', result) : result;
    }, null);
    webGl2Method('getQueryParameter', 2, '-u', [[0,'WebGLQuery',false]], function(query, pname) {
        const id = webGl2Handle(this, query, 'WebGLQuery');
        return id < 0 ? null : webGlCall(this, 'getQueryParameter', [id,pname]);
    }, null);
    for (const [name, signature] of [['samplerParameteri','-ui'], ['samplerParameterf','-uf'], ['getSamplerParameter','-u']]) {
        webGl2Method(name, signature.length, signature, [[0,'WebGLSampler',false]], function(sampler, pname, value) {
            const id = webGl2Handle(this, sampler, 'WebGLSampler');
            if (id < 0) return name === 'getSamplerParameter' ? null : undefined;
            if (name === 'getSamplerParameter') return webGlCall(this, name, [id,pname]);
            if (name === 'samplerParameterf') webGl2Invoke(this, name, [id,pname], [value]);
            else webGl2Invoke(this, name, [id,pname,value]);
        }, name === 'getSamplerParameter' ? null : undefined);
    }
