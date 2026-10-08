// One ordinary public API workload runs unchanged in both realms/reference engines.
function exerciseTransfers() {
    const gl = new OffscreenCanvas(2, 2).getContext('webgl2', {antialias:false});
    if (!gl) throw Error('real native WebGL2 unavailable');
    let checks = 0;
    function equal(actual, expected, label) {
        if (actual.length !== expected.length ||
            Array.from(actual).some((value, index) => value !== expected[index]))
            throw Error(label + ': ' + Array.from(actual));
        checks++;
    }
    function clean(label) {
        const error = gl.getError();
        if (error !== gl.NO_ERROR) throw Error(label + ': GL ' + error);
    }
    const buffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    const original = new ArrayBuffer(20);
    const source = new DataView(original, 4, 8);
    for (let i = 0; i < 8; i++) source.setUint8(i, i + 11);
    gl.bufferData(gl.ARRAY_BUFFER, source, gl.DYNAMIC_DRAW);
    new Uint8Array(original).fill(99);
    const moved = structuredClone(original, {transfer:[original]});
    if (original.byteLength !== 0 || moved.byteLength !== 20)
        throw Error('explicit public transfer did not detach the author source');
    const destination = new Uint8Array(16);
    destination.fill(83);
    gl.getBufferSubData(gl.ARRAY_BUFFER, 0, new DataView(destination.buffer, 4, 8));
    equal(destination, [83,83,83,83,11,12,13,14,15,16,17,18,83,83,83,83], 'DataView native copy');
    clean('DataView transfer');
    const resizable = new ArrayBuffer(16, {maxByteLength:32});
    const resizableView = new Uint8Array(resizable, 4, 8);
    resizableView.set([1,2,3,4,5,6,7,8]);
    gl.bufferData(gl.ARRAY_BUFFER, new Uint8Array(resizableView), gl.DYNAMIC_DRAW);
    resizable.resize(4);
    const copied = new Uint8Array(8);
    gl.getBufferSubData(gl.ARRAY_BUFFER, 0, copied);
    equal(copied, [1,2,3,4,5,6,7,8], 'resize after upload');
    let threw = false;
    try { gl.bufferData(gl.ARRAY_BUFFER, resizableView, gl.DYNAMIC_DRAW); }
    catch (error) { threw = error instanceof TypeError; }
    if (!threw) throw Error('out-of-bounds resizable source accepted');
    resizable.resize(16);
    threw = false;
    try { gl.bufferData(gl.ARRAY_BUFFER, resizableView, gl.DYNAMIC_DRAW); }
    catch (error) { threw = error instanceof TypeError; }
    if (!threw) throw Error('in-bounds resizable source accepted');
    clean('resizable transfer');
    const texture = gl.createTexture();
    const framebuffer = gl.createFramebuffer();
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.bindFramebuffer(gl.FRAMEBUFFER, framebuffer);
    function target(internal, format, type) {
        gl.texImage2D(gl.TEXTURE_2D, 0, internal, 1, 1, 0, format, type, null);
        gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, texture, 0);
        if (gl.checkFramebufferStatus(gl.FRAMEBUFFER) !== gl.FRAMEBUFFER_COMPLETE)
            throw Error('actual typed framebuffer incomplete');
    }
    target(gl.RGBA32UI, gl.RGBA_INTEGER, gl.UNSIGNED_INT);
    const integers = new Uint32Array([0xffffffff,0x80000000,0x12345678,17]);
    gl.clearBufferuiv(gl.COLOR, 0, integers);
    const unsigned = new Uint32Array(10);
    unsigned.fill(83);
    gl.readPixels(0,0,1,1,gl.RGBA_INTEGER,gl.UNSIGNED_INT,unsigned.subarray(1,9),2);
    equal(unsigned, [83,83,83,0xffffffff,0x80000000,0x12345678,17,83,83,83], 'integer reply offsets');
    const wrong = new Float32Array(8);
    wrong.fill(83);
    gl.readPixels(0,0,1,1,gl.RGBA_INTEGER,gl.UNSIGNED_INT,wrong);
    if (gl.getError() !== gl.INVALID_OPERATION) throw Error('wrong pixel view accepted');
    equal(wrong, Array(8).fill(83), 'failed typed reply remains atomic');
    clean('integer replies');
    if (!gl.getExtension('EXT_color_buffer_float')) throw Error('real native float target unavailable');
    target(gl.RGBA32F, gl.RGBA, gl.FLOAT);
    gl.clearBufferfv(gl.COLOR, 0, new Float32Array([-2,4,.5,1]));
    const floats = new Float32Array(10);
    floats.fill(83);
    gl.readPixels(0,0,1,1,gl.RGBA,gl.FLOAT,floats.subarray(1,9),2);
    equal(floats, [83,83,83,-2,4,.5,1,83,83,83], 'HDR reply offsets');
    clean('float replies');
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.clearColor(1,0,0,1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    const packed = gl.createBuffer();
    gl.bindBuffer(gl.PIXEL_PACK_BUFFER, packed);
    gl.bufferData(gl.PIXEL_PACK_BUFFER, new Uint8Array(16).fill(83), gl.DYNAMIC_READ);
    gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,4);
    const mapped = new Uint8Array(16);
    gl.getBufferSubData(gl.PIXEL_PACK_BUFFER, 0, mapped);
    equal(mapped, [83,83,83,83,255,0,0,255,83,83,83,83,83,83,83,83], 'GPU-written PBO truth');
    clean('pixel-pack reply');
    gl.bindBuffer(gl.PIXEL_PACK_BUFFER, null);
    gl.deleteBuffer(packed);
    gl.deleteBuffer(buffer);
    gl.deleteFramebuffer(framebuffer);
    gl.deleteTexture(texture);
    return {checks,error:gl.getError()};
}

if (typeof document !== 'undefined') (async () => {
    const probe = document.querySelector('#probe');
    const windowResult = exerciseTransfers();
    const worker = new Worker('owned-transfer-worker.js');
    let workerResult;
    try {
        workerResult = await new Promise((resolve, reject) => {
            const alarm = setTimeout(() => reject(Error('native Worker contract deadline')), 10000);
            worker.onerror = () => { clearTimeout(alarm); reject(Error('Worker execution failed')); };
            worker.onmessage = event => {
                clearTimeout(alarm);
                if (!event.data.ok) reject(Error(event.data.error));
                else resolve(event.data.result);
            };
            worker.postMessage(null);
        });
    } finally { worker.terminate(); }
    if (windowResult.error || workerResult.error || windowResult.checks !== 6 || workerResult.checks !== 6)
        throw Error('incomplete actual native transfer contracts');
    const result = {window:windowResult,worker:workerResult};
    probe.textContent = JSON.stringify(result);
    probe.setAttribute('data-result', JSON.stringify(result));
    probe.setAttribute('data-state', 'pass');
})().catch(error => {
    const probe = document.querySelector('#probe');
    probe.setAttribute('data-state', 'fail');
    probe.setAttribute('data-error', String(error));
    probe.textContent = String(error);
    throw error;
});
