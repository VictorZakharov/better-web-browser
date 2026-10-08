// Owned workload: timing is evidence, never a feature-admission condition.
try {
    const canvas = document.querySelector('canvas');
    const context = canvas.getContext('2d');
    const probe = document.querySelector('#probe');
    const workloads = {
        brightness: 'brightness(.5)',
        matrices: 'hue-rotate(20deg) saturate(.7) sepia(.2)',
        chain: 'brightness(.7) contrast(1.3) invert(.1) opacity(.8)'
    };
    const results = {};
    for (const [name, filter] of Object.entries(workloads)) {
        canvas.width = 128;
        context.filter = filter;
        context.fillStyle = '#3571b3';
        context.globalCompositeOperation = 'copy';
        context.fillRect(0, 0, 128, 128);
        const start = performance.now();
        for (let iteration = 0; iteration < 24; iteration++)
            context.fillRect(0, 0, 128, 128);
        const pixels = context.getImageData(0, 0, 128, 128).data;
        // Include readback so deferred GPU submission is not mistaken for
        // completed reference-browser rendering.
        const elapsed = performance.now() - start;
        let hash = 2166136261;
        for (const value of pixels) hash = Math.imul(hash ^ value, 16777619) >>> 0;
        results[name] = {elapsed, hash, pixel: Array.from(pixels.slice(0, 4))};
        probe.setAttribute('data-' + name, JSON.stringify(results[name]));
    }
    probe.dataset.done = 'true';
    probe.textContent = JSON.stringify(results, null, 2);
} catch (error) {
    document.querySelector('#probe').dataset.error = String(error.stack || error);
}
