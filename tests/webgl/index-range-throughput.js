// Rasterizer discard isolates repeated geometry submission without filling a
// large framebuffer. Native completion and a final rendered triangle are checked.
(() => {
    const probe = document.querySelector('#probe');
    const gl = document.querySelector('canvas').getContext('webgl2', {antialias:false});
    if (!gl) throw Error('real WebGL2 unavailable');
    const vertex = gl.createShader(gl.VERTEX_SHADER);
    gl.shaderSource(vertex, '#version 300 es\nlayout(location=0) in vec2 position;' +
        'void main(){gl_Position=vec4(position,0.,1.);}');
    gl.compileShader(vertex);
    const fragment = gl.createShader(gl.FRAGMENT_SHADER);
    gl.shaderSource(fragment, '#version 300 es\nprecision mediump float;' +
        'out vec4 color;void main(){color=vec4(0,0,1,1);}');
    gl.compileShader(fragment);
    const program = gl.createProgram();
    gl.attachShader(program, vertex);
    gl.attachShader(program, fragment);
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS))
        throw Error('actual link failed: ' + gl.getProgramInfoLog(program));
    gl.useProgram(program);
    const vertices = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, vertices);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1,-1,3,-1,-1,3]), gl.STATIC_DRAW);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    gl.enableVertexAttribArray(0);
    const count = 120000;
    const indices = new Uint16Array(count);
    for (let i = 0; i < count; i++) indices[i] = i % 3;
    const elements = gl.createBuffer();
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, elements);
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, indices, gl.DYNAMIC_DRAW);
    gl.enable(gl.RASTERIZER_DISCARD);
    gl.drawElements(gl.TRIANGLES, count, gl.UNSIGNED_SHORT, 0);
    gl.finish();
    const draws = 100;
    const start = performance.now();
    for (let i = 0; i < draws; i++)
        gl.drawElements(gl.TRIANGLES, count, gl.UNSIGNED_SHORT, 0);
    const submittedMs = performance.now() - start;
    gl.finish();
    gl.disable(gl.RASTERIZER_DISCARD);
    gl.drawElements(gl.TRIANGLES, 3, gl.UNSIGNED_SHORT, 0);
    const pixels = new Uint8Array(64);
    gl.readPixels(0, 0, 4, 4, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
    const completedMs = performance.now() - start;
    for (let i = 0; i < pixels.length; i += 4) {
        if (pixels[i] !== 0 || pixels[i+1] !== 0 || pixels[i+2] !== 255 || pixels[i+3] !== 255)
            throw Error('real indexed triangle pixels differ');
    }
    // Both allowed vertex-bounds policies remain possible here: Breeze rejects
    // an out-of-range vertex, while Chrome may use robust zero/in-buffer fetches.
    // Breeze's exact content invalidation is asserted separately in native tests.
    gl.bufferSubData(gl.ELEMENT_ARRAY_BUFFER, 0, new Uint16Array([99]));
    gl.drawElements(gl.TRIANGLES, count, gl.UNSIGNED_SHORT, 0);
    const boundsError = gl.getError();
    if (boundsError !== gl.INVALID_OPERATION && boundsError !== gl.NO_ERROR)
        throw Error('unexpected vertex-bounds error ' + boundsError);
    // Unlike vertex fetches, out-of-bounds index-buffer fetches MUST error.
    // Shrinking storage cannot reuse the cached admitted range in either browser.
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, 2, gl.DYNAMIC_DRAW);
    gl.drawElements(gl.TRIANGLES, count, gl.UNSIGNED_SHORT, 0);
    if (gl.getError() !== gl.INVALID_OPERATION)
        throw Error('cached range bypassed current index-buffer extent');
    const error = gl.getError();
    if (error !== gl.NO_ERROR) throw Error('unexpected native error: ' + error);
    gl.useProgram(null);
    gl.deleteProgram(program);
    gl.deleteShader(vertex);
    gl.deleteShader(fragment);
    gl.deleteBuffer(vertices);
    gl.deleteBuffer(elements);
    const result = {draws,count,bytes:indices.byteLength,programs:1,submittedMs,completedMs,boundsError,error};
    probe.textContent = JSON.stringify(result);
    probe.setAttribute('data-result', JSON.stringify(result));
    probe.setAttribute('data-state', 'pass');
})();
