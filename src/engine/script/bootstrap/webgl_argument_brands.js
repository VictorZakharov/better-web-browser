    // Interface conversion precedes the WebGL lost-context early return. Checking
    // brands here (not native IDs) also keeps arbitrary author objects out of IPC.
    // https://registry.khronos.org/webgl/specs/latest/1.0/webgl.idl
    const webGlInterfaceArguments = new Map();
    const webGlInterfaceSignature = (names, ...entries) => {
        for (const name of names) webGlInterfaceArguments.set(name, entries);
    };
    for (const type of ['Buffer', 'Shader', 'Program', 'Texture', 'Framebuffer', 'Renderbuffer']) {
        webGlInterfaceSignature(['delete' + type, 'is' + type], [0, 'WebGL' + type, true]);
    }
    for (const type of ['Buffer', 'Texture', 'Framebuffer', 'Renderbuffer']) {
        webGlInterfaceSignature(['bind' + type], [1, 'WebGL' + type, true]);
    }
    webGlInterfaceSignature(['compileShader', 'shaderSource', 'getShaderParameter',
        'getShaderInfoLog', 'getShaderSource'], [0, 'WebGLShader', false]);
    webGlInterfaceSignature(['linkProgram', 'validateProgram', 'getProgramParameter',
        'getProgramInfoLog', 'getAttachedShaders', 'getAttribLocation', 'bindAttribLocation',
        'getUniformLocation', 'getActiveUniform', 'getActiveAttrib'], [0, 'WebGLProgram', false]);
    webGlInterfaceSignature(['useProgram'], [0, 'WebGLProgram', true]);
    webGlInterfaceSignature(['attachShader', 'detachShader'],
        [0, 'WebGLProgram', false], [1, 'WebGLShader', false]);
    webGlInterfaceSignature(['getUniform'],
        [0, 'WebGLProgram', false], [1, 'WebGLUniformLocation', false]);
    webGlInterfaceSignature(['framebufferTexture2D'], [3, 'WebGLTexture', true]);
    webGlInterfaceSignature(['framebufferRenderbuffer'], [3, 'WebGLRenderbuffer', true]);
    for (let count = 1; count <= 4; count++) for (const kind of ['f', 'i']) {
        webGlInterfaceSignature(['uniform' + count + kind, 'uniform' + count + kind + 'v'],
            [0, 'WebGLUniformLocation', true]);
    }
    for (let count = 2; count <= 4; count++) {
        webGlInterfaceSignature(['uniformMatrix' + count + 'fv'], [0, 'WebGLUniformLocation', true]);
    }
    const webGlConvertInterface = (args, index, type, nullable) => {
            const value = args[index];
            if (nullable && (value === null || value === undefined)) {
                args[index] = null;
                return;
            }
            if (webGlObjects.get(value)?.type !== type) throw new TypeError('Expected ' + type);
    };
