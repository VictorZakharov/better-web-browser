'use strict';
(() => {
    const canvas = document.querySelector('#canvas');
    // Repeated readback is the workload. Request CPU-oriented storage from the
    // start, rather than comparing a reference browser's GPU-to-CPU migration.
    const context = canvas.getContext('2d', {willReadFrequently: true});
    const probe = document.querySelector('#probe');
    const measurements = [], checksums = [];
    let batch = 0;
    function run() {
        canvas.width = 512;
        const start = performance.now();
        for (let index = 0; index < 16; index++) {
            context.strokeStyle = index % 2 ? 'rgba(17,99,211,0.7)' : 'rgba(219,83,29,0.4)';
            context.lineWidth = 1 + index % 4;
            context.lineJoin = 'round';
            context.lineCap = 'round';
            context.globalAlpha = 0.73;
            context.beginPath();
            context.moveTo(0, 0);
            context.bezierCurveTo(490 - index * 9, 490, index * 11, 25, 512, 512);
            context.stroke();
        }
        // Fence real pixels, including any implementation's deferred rendering.
        const pixels = context.getImageData(0, 0, 512, 512).data;
        measurements.push(performance.now() - start);
        let checksum = 0;
        for (let index = 0; index < pixels.length; index++)
            checksum = (Math.imul(checksum, 31) + pixels[index]) >>> 0;
        checksums.push(checksum);
        if (batch && checksums[batch] !== checksums[0]) throw Error('unstable Canvas pixels');
        probe.setAttribute('data-measurements', JSON.stringify(measurements));
        probe.setAttribute('data-checksums', JSON.stringify(checksums));
        probe.textContent = JSON.stringify({measurements, checksums});
        if (++batch < 4) setTimeout(run, 0);
        else probe.setAttribute('data-done', 'true');
    }
    setTimeout(run, 0);
})();
