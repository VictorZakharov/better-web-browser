// Native compile/link work completes before location timing starts. Every
// reflected location is then used, queried and checked through native pixels.
(async () => {
    const probe = document.querySelector('#probe');
    const gl = document.querySelector('canvas').getContext('webgl2', {antialias:false});
    if (!gl) throw Error('real WebGL2 context unavailable');
    const extension = gl.getExtension('KHR_parallel_shader_compile');
    if (!extension) throw Error('native completion queries unavailable');
    const uniforms = 96;
    const declarations = Array.from({length:uniforms}, (_, i) =>
        'uniform float value' + i + ';').join('');
    const expression = Array.from({length:uniforms}, (_, i) => 'value' + i).join('+');
    const programs = [];
    for (let variant = 0; variant < 8; variant++) {
        const vertex = gl.createShader(gl.VERTEX_SHADER);
        const fragment = gl.createShader(gl.FRAGMENT_SHADER);
        gl.shaderSource(vertex, '#version 300 es\nvoid main(){' +
            'vec2 p=vec2((gl_VertexID<<1)&2,gl_VertexID&2);gl_Position=vec4(p*2.-1.,0,1);}');
        gl.shaderSource(fragment, '#version 300 es\nprecision highp float;' +
            declarations + 'out vec4 color;void main(){color=vec4((' + expression +
            ')/float(' + uniforms + '),1.,0.,1.);}');
        gl.compileShader(vertex);
        gl.compileShader(fragment);
        const program = gl.createProgram();
        gl.attachShader(program, vertex);
        gl.attachShader(program, fragment);
        gl.linkProgram(program);
        gl.deleteShader(vertex);
        gl.deleteShader(fragment);
        programs.push(program);
    }
    const deadline = performance.now() + 30000;
    while (programs.some(program =>
        !gl.getProgramParameter(program, extension.COMPLETION_STATUS_KHR))) {
        if (performance.now() > deadline) throw Error('native compiler deadline');
        await new Promise(resolve => setTimeout(resolve, 10));
    }
    for (const program of programs) {
        if (!gl.getProgramParameter(program, gl.LINK_STATUS))
            throw Error('actual link failed: ' + gl.getProgramInfoLog(program));
        if (gl.getProgramParameter(program, gl.ACTIVE_UNIFORMS) !== uniforms)
            throw Error('not all declared uniforms are actually active');
    }
    const start = performance.now();
    const locations = programs.map(program =>
        Array.from({length:uniforms}, (_, i) => {
            const location = gl.getUniformLocation(program, 'value' + i);
            if (location === null) throw Error('missing actual native location');
            return location;
        }));
    const submittedMs = performance.now() - start;
    for (let i = 0; i < programs.length; i++) {
        const program = programs[i];
        gl.useProgram(program);
        for (const location of locations[i]) gl.uniform1f(location, .25);
        for (const location of locations[i]) {
            if (gl.getUniform(program, location) !== .25)
                throw Error('reflected scalar type or native value mismatch');
        }
        gl.drawArrays(gl.TRIANGLES, 0, 3);
        const pixel = new Uint8Array(4);
        gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);
        if (pixel[0] !== 64 || pixel[1] !== 255 || pixel[2] !== 0 || pixel[3] !== 255)
            throw Error('program did not actually render: ' + Array.from(pixel));
    }
    const completedMs = performance.now() - start;
    const error = gl.getError();
    if (error !== gl.NO_ERROR) throw Error('native uniform error: ' + error);
    gl.useProgram(null);
    for (const program of programs) gl.deleteProgram(program);
    const result = {programs:programs.length,uniforms,locations:programs.length*uniforms,
        submittedMs,completedMs,error};
    probe.textContent = JSON.stringify(result);
    probe.setAttribute('data-result', JSON.stringify(result));
    probe.setAttribute('data-state', 'pass');
})().catch(error => {
    document.querySelector('#probe').setAttribute('data-state', 'fail');
    document.querySelector('#probe').textContent = String(error);
    throw error;
});
