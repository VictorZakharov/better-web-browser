// Owned shader workload: real compiler submission, non-blocking completion polling,
// success queries and exact readback. No dependency on a particular game's source.
(async () => {
    const probe = document.querySelector('#probe');
    const gl = document.querySelector('canvas').getContext('webgl2', {antialias:false});
    if (!gl) throw Error('real WebGL2 context unavailable');
    const extension = gl.getExtension('KHR_parallel_shader_compile');
    if (!extension) throw Error('parallel completion extension unavailable');
    const programs = [];
    const start = performance.now();
    for (let variant = 0; variant < 24; variant++) {
        const vertex = gl.createShader(gl.VERTEX_SHADER);
        const fragment = gl.createShader(gl.FRAGMENT_SHADER);
        gl.shaderSource(vertex, '#version 300 es\nvoid main(){' +
            'vec2 p=vec2((gl_VertexID<<1)&2,gl_VertexID&2);gl_Position=vec4(p*2.-1.,0,1);}');
        // Uniform-dependent branches cannot simply become compile-time constants.
        const operations = Array.from({length:64}, (_, index) =>
            'v=sin(v*1.01+float(' + (index + variant) + ')*.001)+v*.7;').join('');
        gl.shaderSource(fragment, '#version 300 es\nprecision highp float;' +
            'uniform float seed;out vec4 color;void main(){float v=seed;' + operations +
            'color=vec4(clamp(v,0.,1.),1.,0.,1.);}');
        gl.compileShader(vertex);
        gl.compileShader(fragment);
        const program = gl.createProgram();
        gl.attachShader(program, vertex);
        gl.attachShader(program, fragment);
        gl.linkProgram(program);
        // ANGLE must retain a pending link's attached shaders until native work completes.
        gl.deleteShader(vertex);
        gl.deleteShader(fragment);
        programs.push(program);
    }
    const submittedMs = performance.now() - start;
    let polls = 0;
    let pendingObserved = false;
    while (true) {
        let complete = true;
        for (const program of programs) {
            if (!gl.getProgramParameter(program, extension.COMPLETION_STATUS_KHR)) complete = false;
        }
        polls++;
        if (complete) break;
        pendingObserved = true;
        if (performance.now() - start > 30000) throw Error('bounded fixture compiler deadline');
        await new Promise(resolve => setTimeout(resolve, 10));
    }
    const completedMs = performance.now() - start;
    for (const program of programs) {
        if (!gl.getProgramParameter(program, gl.LINK_STATUS))
            throw Error('native link failed: ' + gl.getProgramInfoLog(program));
        gl.useProgram(program);
        gl.uniform1f(gl.getUniformLocation(program, 'seed'), .25);
        gl.drawArrays(gl.TRIANGLES, 0, 3);
        const pixel = new Uint8Array(4);
        gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);
        if (pixel[1] !== 255 || pixel[2] !== 0 || pixel[3] !== 255)
            throw Error('material did not actually render: ' + Array.from(pixel));
    }
    const error = gl.getError();
    if (error !== gl.NO_ERROR) throw Error('native warmup error: ' + error);
    for (const program of programs) gl.deleteProgram(program);
    gl.useProgram(null);
    const result = {programs:programs.length,submittedMs,completedMs,polls,pendingObserved,error};
    probe.textContent = JSON.stringify(result);
    probe.setAttribute('data-result', JSON.stringify(result));
    probe.setAttribute('data-state', 'pass');
})().catch(error => {
    document.querySelector('#probe').setAttribute('data-state', 'fail');
    document.querySelector('#probe').textContent = String(error);
    throw error;
});
