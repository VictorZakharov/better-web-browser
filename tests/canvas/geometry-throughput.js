'use strict';
(() => {
    const canvas = document.querySelector('#canvas');
    const context = canvas.getContext('2d', {willReadFrequently: true});
    const probe = document.querySelector('#probe');
    const path = new Path2D();
    // Keep raster area small and geometry moderately dense. Both browsers run
    // the same ordinary API calls; no private host command or timing shortcut.
    for (let i = 0; i < 512; i++) {
        const angle = i * Math.PI * 2 / 512;
        const radius = 23 + 4 * Math.sin(angle * 11);
        const x = 32 + Math.cos(angle) * radius;
        const y = 32 + Math.sin(angle) * radius;
        if (i === 0) path.moveTo(x, y); else path.lineTo(x, y);
    }
    path.closePath();
    const measurements = [], checksums = [];
    function run() {
        canvas.width = 64;
        context.fillStyle = 'rgba(197,43,239,.7)';
        context.strokeStyle = 'rgba(17,99,211,.4)';
        context.globalAlpha = .63;
        context.lineWidth = 1.25;
        const start = performance.now();
        for (let i = 0; i < 32; i++) {
            context.fill(path, i % 2 ? 'evenodd' : 'nonzero');
            context.stroke(path);
        }
        // Fence all painted pixels before reporting elapsed time.
        const pixels = context.getImageData(0, 0, 64, 64).data;
        measurements.push(performance.now() - start);
        let checksum = 0;
        for (let i = 0; i < pixels.length; i++)
            checksum = (Math.imul(checksum, 31) + pixels[i]) >>> 0;
        checksums.push(checksum);
        if (checksums.length > 1 && checksum !== checksums[0]) throw Error('unstable Canvas pixels');
        probe.setAttribute('data-measurements', JSON.stringify(measurements));
        probe.setAttribute('data-checksums', JSON.stringify(checksums));
        probe.textContent = JSON.stringify({measurements, checksums});
        if (measurements.length < 4) setTimeout(run, 0);
        else probe.setAttribute('data-done', 'true');
    }
    setTimeout(run, 0);
})();
