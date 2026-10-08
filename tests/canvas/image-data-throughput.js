'use strict';
(() => {
    const canvas = document.querySelector('#canvas');
    const context = canvas.getContext('2d', {willReadFrequently: true});
    const probe = document.querySelector('#probe');
    const image = new ImageData(512, 512);
    for (let i = 0; i < image.data.length; i++) image.data[i] = i % 4 === 3 ? 255 : (i * 17 + 13) % 256;
    const measurements = [], checksums = [];
    function run() {
        canvas.width = 512;
        let source = image;
        const start = performance.now();
        for (let i = 0; i < 12; i++) {
            context.putImageData(source, 0, 0);
            source = context.getImageData(0, 0, 512, 512);
        }
        measurements.push(performance.now() - start);
        let checksum = 0;
        for (let i = 0; i < source.data.length; i++) {
            if (source.data[i] !== image.data[i]) throw Error('ImageData pixels changed');
            checksum = (Math.imul(checksum, 31) + source.data[i]) >>> 0;
        }
        checksums.push(checksum);
        probe.setAttribute('data-measurements', JSON.stringify(measurements));
        probe.setAttribute('data-checksums', JSON.stringify(checksums));
        probe.textContent = JSON.stringify({measurements, checksums});
        if (measurements.length < 4) setTimeout(run, 0);
        else probe.setAttribute('data-done', 'true');
    }
    setTimeout(run, 0);
})();
