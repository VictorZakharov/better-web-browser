onmessage = event => {
    const {width,height,sequence} = event.data;
    const start = performance.now();
    const pixels = new Uint8ClampedArray(width * height * 4);
    let checksum = 0;
    for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
        let nearest = Infinity;
        for (let oy = -1; oy <= 1; oy++) for (let ox = -1; ox <= 1; ox++) {
            const dx = (x % 31) / 31 - ox - .37;
            const dy = (y % 29) / 29 - oy - .61;
            nearest = Math.min(nearest, Math.hypot(dx, dy));
        }
        const offset = (y * width + x) * 4;
        pixels[offset] = nearest * 255;
        pixels[offset + 1] = (Math.sin(x / 17) * Math.cos(y / 13) + 1) * 127;
        pixels[offset + 2] = Math.sqrt(nearest) * 255;
        pixels[offset + 3] = 255;
        checksum = (checksum + pixels[offset] + pixels[offset + 1] + pixels[offset + 2]) >>> 0;
    }
    postMessage({width,height,sequence,elapsed:performance.now()-start,checksum,pixels}, [pixels.buffer]);
};
