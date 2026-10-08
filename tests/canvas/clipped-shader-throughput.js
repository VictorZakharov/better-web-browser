'use strict';
(() => {
    const canvas = document.querySelector('#canvas');
    const c = canvas.getContext('2d', {willReadFrequently: true});
    const probe = document.querySelector('#probe');
    const kinds = ['linear', 'radial', 'pattern'];
    const measurements = {}, checksums = {};
    const source = document.createElement('canvas');
    source.width = source.height = 2;
    const s = source.getContext('2d');
    s.fillStyle = 'red'; s.fillRect(0, 0, 1, 2);
    s.fillStyle = 'blue'; s.fillRect(1, 0, 1, 2);
    let kindIndex = 0, batch = 0;
    function run() {
        canvas.width = 128;
        const kind = kinds[kindIndex];
        let paint;
        if (kind === 'pattern') paint = c.createPattern(source, 'repeat');
        else {
            paint = kind === 'linear' ? c.createLinearGradient(0, 0, 128, 64)
                : c.createRadialGradient(64, 64, 0, 64, 64, 80);
            paint.addColorStop(0, 'red'); paint.addColorStop(1, 'blue');
        }
        c.beginPath(); c.rect(8, 8, 112, 112); c.rect(40, 40, 48, 48);
        c.clip('evenodd'); c.fillStyle = paint;
        const start = performance.now();
        for (let index = 0; index < 8; index++) c.fillRect(index, index, 128, 128);
        const pixels = c.getImageData(0, 0, 128, 128).data;
        const elapsed = performance.now() - start;
        let checksum = 0;
        for (let i = 0; i < pixels.length; i++)
            checksum = (Math.imul(checksum, 31) + pixels[i]) >>> 0;
        // Check every outside/hole pixel independently of shader interpolation.
        for (let y = 0; y < 128; y++) for (let x = 0; x < 128; x++) {
            const inside = x >= 8 && x < 120 && y >= 8 && y < 120
                && !(x >= 40 && x < 88 && y >= 40 && y < 88);
            const i = (y * 128 + x) * 4;
            if (!inside && pixels.slice(i, i + 4).some(value => value !== 0))
                throw Error('Clip escaped: ' + kind + '/' + x + ',' + y);
            if (inside && pixels[i + 3] !== 255) throw Error('Missing opaque shader');
        }
        if (!batch) { measurements[kind] = []; checksums[kind] = checksum; }
        else {
            if (checksums[kind] !== checksum) throw Error('Unstable shader pixels');
            measurements[kind].push(elapsed);
        }
        probe.setAttribute('data-measurements', JSON.stringify(measurements));
        probe.setAttribute('data-checksums', JSON.stringify(checksums));
        if (++batch === 4) { batch = 0; kindIndex++; }
        if (kindIndex < kinds.length) setTimeout(run, 0);
        else { probe.textContent = JSON.stringify(measurements); probe.setAttribute('data-done', 'true'); }
    }
    setTimeout(run, 0);
})();
