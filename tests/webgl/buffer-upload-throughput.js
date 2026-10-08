// This measures repeated replacement of one buffer, not increasing GPU limits.
// The author view remains independently mutable after every synchronous upload.
(() => {
    const probe = document.querySelector('#probe');
    const gl = document.querySelector('canvas').getContext('webgl2', {antialias:false});
    if (!gl) throw Error('real WebGL2 unavailable');
    const buffer = gl.createBuffer();
    const bytes = 4 * 1024 * 1024;
    const storage = new ArrayBuffer(bytes + 32);
    const source = new Uint8Array(storage, 16, bytes);
    source.fill(93);
    const vertices = new Float32Array(storage, 16, 6);
    vertices.set([-1,-1,3,-1,-1,3]);
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, source, gl.DYNAMIC_DRAW);
    gl.finish();
    function compile(type, text) {
        const shader = gl.createShader(type);
        gl.shaderSource(shader, text);
        gl.compileShader(shader);
        return shader;
    }
    const vertex = compile(gl.VERTEX_SHADER, '#version 300 es\n' +
        'in vec2 position;void main(){gl_Position=vec4(position,0,1);}');
    const fragment = compile(gl.FRAGMENT_SHADER, '#version 300 es\n' +
        'precision mediump float;out vec4 color;void main(){color=vec4(0,1,0,1);}');
    const program = gl.createProgram();
    gl.attachShader(program, vertex);
    gl.attachShader(program, fragment);
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS))
        throw Error('actual link failed: ' + gl.getProgramInfoLog(program));
    gl.useProgram(program);
    const location = gl.getAttribLocation(program, 'position');
    gl.vertexAttribPointer(location, 2, gl.FLOAT, false, 0, 0);
    gl.enableVertexAttribArray(location);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    const pixels = new Uint8Array(64);
    gl.readPixels(0, 0, 4, 4, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
    // Compile/link and the first actual native upload/draw finish before timing.
    const uploads = 32;
    const start = performance.now();
    for (let i = 0; i < uploads; i++)
        gl.bufferData(gl.ARRAY_BUFFER, source, gl.DYNAMIC_DRAW);
    const submittedMs = performance.now() - start;
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.readPixels(0, 0, 4, 4, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
    const completedMs = performance.now() - start;
    // Neither the author buffer nor the nonzero-offset view may be detached.
    if (storage.byteLength !== bytes + 32 || source.length !== bytes)
        throw Error('upload took ownership of author memory');
    source.fill(0);
    const sampled = new Uint8Array(16);
    gl.getBufferSubData(gl.ARRAY_BUFFER, bytes - 16, sampled);
    if (sampled.some(value => value !== 93))
        throw Error('native storage changed with later author mutation');
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.readPixels(0, 0, 4, 4, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
    for (let i = 0; i < pixels.length; i += 4) {
        if (pixels[i] !== 0 || pixels[i+1] !== 255 || pixels[i+2] !== 0 || pixels[i+3] !== 255)
            throw Error('native vertex fetch changed with later source mutation');
    }
    const error = gl.getError();
    if (error !== gl.NO_ERROR) throw Error('native upload error: ' + error);
    gl.useProgram(null);
    gl.deleteProgram(program);
    gl.deleteShader(vertex);
    gl.deleteShader(fragment);
    gl.deleteBuffer(buffer);
    const result = {uploads,bytes,programs:1,submittedMs,completedMs,error};
    probe.textContent = JSON.stringify(result);
    probe.setAttribute('data-result', JSON.stringify(result));
    probe.setAttribute('data-state', 'pass');
})()
