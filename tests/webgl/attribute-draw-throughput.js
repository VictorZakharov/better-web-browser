// Eight genuine active inputs. Compilation and initial reflection finish before
// timing; every timed draw must still validate its current buffers and VAO.
(async () => {
    const probe = document.querySelector('#probe');
    const gl = document.querySelector('canvas').getContext('webgl2', {antialias:false});
    if (!gl) throw Error('real WebGL2 unavailable');
    const count = 8;
    const declarations = Array.from({length:count}, (_, i) =>
        'layout(location=' + i + ') in vec2 input' + i + ';').join('');
    const expression = Array.from({length:count}, (_, i) => 'input' + i).join('+');
    function compile(type, source) {
        const shader = gl.createShader(type);
        gl.shaderSource(shader, source);
        gl.compileShader(shader);
        return shader;
    }
    const vertex = compile(gl.VERTEX_SHADER, '#version 300 es\n' + declarations +
        'void main(){gl_Position=vec4((' + expression + ')/8.,0.,1.);}');
    const fragment = compile(gl.FRAGMENT_SHADER, '#version 300 es\n' +
        'precision mediump float;out vec4 color;void main(){color=vec4(1,0,0,1);}');
    const program = gl.createProgram();
    gl.attachShader(program, vertex);
    gl.attachShader(program, fragment);
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS))
        throw Error('actual link failed: ' + gl.getProgramInfoLog(program));
    if (gl.getProgramParameter(program, gl.ACTIVE_ATTRIBUTES) !== count)
        throw Error('all eight native inputs must be active');
    const buffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1,-1,3,-1,-1,3]), gl.STATIC_DRAW);
    for (let i = 0; i < count; i++) {
        gl.vertexAttribPointer(i, 2, gl.FLOAT, false, 0, 0);
        gl.enableVertexAttribArray(i);
    }
    gl.useProgram(program);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.finish();
    const draws = 2000;
    const start = performance.now();
    for (let i = 0; i < draws; i++) gl.drawArrays(gl.TRIANGLES, 0, 3);
    const submittedMs = performance.now() - start;
    gl.finish();
    const pixels = new Uint8Array(4 * 4 * 4);
    gl.readPixels(0, 0, 4, 4, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
    const completedMs = performance.now() - start;
    for (let i = 0; i < pixels.length; i += 4) {
        if (pixels[i] !== 255 || pixels[i+1] !== 0 || pixels[i+2] !== 0 || pixels[i+3] !== 255)
            throw Error('actual native draw pixels differ');
    }
    // Cached reflection cannot allow a later undersized backing store.
    gl.bufferData(gl.ARRAY_BUFFER, 8, gl.STATIC_DRAW);
    gl.clearColor(0, 1, 0, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    const boundsError = gl.getError();
    // WebGL permits either INVALID_OPERATION or robust zero/in-buffer fetches.
    // Chrome uses the latter; Breeze's stricter policy has native unit coverage.
    if (boundsError !== gl.INVALID_OPERATION && boundsError !== gl.NO_ERROR)
        throw Error('changed vertex bounds: unexpected error ' + boundsError);
    if (boundsError === gl.INVALID_OPERATION) {
        gl.readPixels(0, 0, 4, 4, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
        for (let i = 0; i < pixels.length; i += 4) {
            if (pixels[i] !== 0 || pixels[i+1] !== 255 || pixels[i+2] !== 0 || pixels[i+3] !== 255)
                throw Error('rejected draw changed its framebuffer');
        }
    }
    const error = gl.getError();
    if (error !== gl.NO_ERROR) throw Error('unexpected native error: ' + error);
    gl.useProgram(null);
    gl.deleteProgram(program);
    gl.deleteShader(vertex);
    gl.deleteShader(fragment);
    gl.deleteBuffer(buffer);
    const result = {draws,attributes:count,programs:1,submittedMs,completedMs,boundsError,error};
    probe.textContent = JSON.stringify(result);
    probe.setAttribute('data-result', JSON.stringify(result));
    probe.setAttribute('data-state', 'pass');
})().catch(error => {
    document.querySelector('#probe').setAttribute('data-state', 'fail');
    document.querySelector('#probe').setAttribute('data-error', String(error));
    document.querySelector('#probe').textContent = String(error);
    throw error;
});
