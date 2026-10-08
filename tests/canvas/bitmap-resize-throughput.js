// Timing covers completed resize and draw/readback, not just promise creation.
(async () => {
    const source = new ImageData(64, 64);
    for (let index = 0; index < source.data.length; index++)
        source.data[index] = (index * 79 + 31) % 256;
    const canvas = document.querySelector('canvas');
    const context = canvas.getContext('2d');
    const probe = document.querySelector('#probe');
    const workloads = [
        ['enlarge', 256, 256, 'low'],
        ['pixelated', 150, 150, 'pixelated'],
        ['reduce', 19, 23, 'low']
    ];
    for (const [name, width, height, quality] of workloads) {
        canvas.width = width;
        canvas.height = height;
        context.globalCompositeOperation = 'copy';
        context.imageSmoothingEnabled = false;
        const start = performance.now();
        for (let iteration = 0; iteration < 12; iteration++) {
            const bitmap = await createImageBitmap(source, {
                resizeWidth: width, resizeHeight: height, resizeQuality: quality,
                premultiplyAlpha: 'none'
            });
            context.drawImage(bitmap, 0, 0);
            bitmap.close();
        }
        const pixels = context.getImageData(0, 0, width, height).data;
        const elapsed = performance.now() - start;
        let hash = 2166136261;
        for (const value of pixels) hash = Math.imul(hash ^ value, 16777619) >>> 0;
        probe.setAttribute('data-' + name, JSON.stringify({elapsed, hash}));
    }
    probe.dataset.done = 'true';
    probe.textContent = 'completed';
})().catch(error => {
    document.querySelector('#probe').dataset.error = String(error.stack || error);
});
