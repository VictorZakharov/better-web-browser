// Original standards acceptance fixture; no GPU/driver names or timed pass rules.
(() => {
    'use strict';
    const output = document.getElementById('results');
    const assert = (condition, message) => { if (!condition) throw Error(message); };
    const run = (name, test) => {
        const element = document.createElement('p');
        element.className = 'contract';
        try {
            test(); element.dataset.result = 'pass'; element.textContent = name + ': pass';
        } catch (error) {
            element.dataset.result = 'fail'; element.textContent = name + ': ' + error.message;
        }
        output.appendChild(element);
    };
    const readGreen = gl => {
        gl.clearColor(0, 1, 0, 1); gl.clear(gl.COLOR_BUFFER_BIT);
        const pixels = new Uint8Array(4); gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
        assert([...pixels].join() === '0,255,0,255' && gl.getError() === 0, 'native pixels unavailable');
    };
    for (const type of ['webgl', 'webgl2']) {
        for (const preference of ['default', 'low-power', 'high-performance']) {
            run(type + ' / ' + preference, () => {
                const canvas = document.createElement('canvas'); canvas.width = canvas.height = 2;
                const gl = canvas.getContext(type, {powerPreference:preference, antialias:false});
                assert(gl, 'no usable real backend');
                try {
                    readGreen(gl);
                    const attributes = gl.getContextAttributes();
                    assert(attributes.powerPreference === preference, 'requested hint not retained');
                    assert(!attributes.failIfMajorPerformanceCaveat, 'strict caveat unexpectedly requested');
                    const limit = gl.getParameter(gl.MAX_COMBINED_TEXTURE_IMAGE_UNITS);
                    assert(Number.isInteger(limit) && limit >= 8, 'invalid admitted texture limit');
                    gl.activeTexture(gl.TEXTURE0 + limit - 1);
                    assert(gl.getError() === 0, 'reported last texture unit is unavailable');
                    gl.activeTexture(gl.TEXTURE0 + limit);
                    assert(gl.getError() === gl.INVALID_ENUM, 'unreported texture unit admitted');
                } finally { gl.getExtension('WEBGL_lose_context').loseContext(); }
            });
        }
        run(type + ' / strict caveat', () => {
            const canvas = document.createElement('canvas'); canvas.width = canvas.height = 2;
            let failures = 0;
            canvas.addEventListener('webglcontextcreationerror', () => failures++);
            const gl = canvas.getContext(type, {failIfMajorPerformanceCaveat:true, antialias:false});
            if (gl) {
                try {
                    readGreen(gl);
                    assert(gl.getContextAttributes().failIfMajorPerformanceCaveat, 'strict request lost');
                    output.dataset.strictHardware = 'available';
                } finally { gl.getExtension('WEBGL_lose_context').loseContext(); }
            } else {
                assert(failures === 1, 'rejected backend did not report creation failure');
                const fallback = canvas.getContext(type, {antialias:false});
                assert(fallback, 'strict failure consumed canvas context mode');
                try { readGreen(fallback); } finally { fallback.getExtension('WEBGL_lose_context').loseContext(); }
                output.dataset.strictHardware = 'unavailable';
            }
        });
    }
    output.dataset.complete = 'true';
})();
