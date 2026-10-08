'use strict';
function testImageDataRectangles(make) {
    let assertions = 0;
    const equal = (actual, expected, message) => {
        assertions++;
        if (actual.length !== expected.length) throw Error(message + ': length');
        for (let i = 0; i < expected.length; i++)
            if (actual[i] !== expected[i]) throw Error(message + ': byte ' + i);
    };
    for (const [sourceWidth, sourceHeight, width, height] of [
        [7, 5, 7, 8], [7, 8, 7, 5], [5, 7, 9, 6], [9, 6, 5, 7], [1, 7, 1, 9]
    ]) {
        const canvas = make(width, height), context = canvas.getContext('2d');
        const source = new ImageData(sourceWidth, sourceHeight);
        for (let i = 0; i < source.data.length; i++) source.data[i] = i % 4 === 3 ? 255 : (i * 17 + 13) % 256;
        for (const [dx, dy, dirtyX, dirtyY, dirtyWidth, dirtyHeight] of [
            [0, 0, 0, 0, sourceWidth, sourceHeight], [0, -2, 0, 0, sourceWidth, sourceHeight],
            [0, 2, 0, 0, sourceWidth, sourceHeight], [0, 0, 0, 1, sourceWidth, 3],
            [0, 0, sourceWidth, sourceHeight, -sourceWidth, -sourceHeight],
            [0, 0, 1, 0, sourceWidth - 2, sourceHeight], [-2, 1, 0, 0, sourceWidth, sourceHeight],
            [2, 1, -2, -1, sourceWidth + 4, sourceHeight + 2], [0, 0, 0, 0, 0, sourceHeight]
        ]) {
            canvas.width = width;
            context.putImageData(source, dx, dy, dirtyX, dirtyY, dirtyWidth, dirtyHeight);
            const expected = new Uint8ClampedArray(width * height * 4);
            const minX = Math.min(dirtyX, dirtyX + dirtyWidth), maxX = Math.max(dirtyX, dirtyX + dirtyWidth);
            const minY = Math.min(dirtyY, dirtyY + dirtyHeight), maxY = Math.max(dirtyY, dirtyY + dirtyHeight);
            for (let sy = 0; sy < sourceHeight; sy++) for (let sx = 0; sx < sourceWidth; sx++) {
                const x = sx + dx, y = sy + dy;
                if (sx < minX || sx >= maxX || sy < minY || sy >= maxY || x < 0 || x >= width || y < 0 || y >= height) continue;
                for (let channel = 0; channel < 4; channel++)
                    expected[(y * width + x) * 4 + channel] = source.data[(sy * sourceWidth + sx) * 4 + channel];
            }
            equal(context.getImageData(0, 0, width, height).data, expected, 'write intersection');
            for (const [rx0, ry0, rw0, rh0] of [
                [0, 0, width, height], [0, -2, width, height + 4], [0, 1, width, 3],
                [-2, -1, width + 4, height + 2], [width, height, -width, -height], [width + 1, 0, 2, 2]
            ]) {
                const rx = rw0 < 0 ? rx0 + rw0 : rx0, ry = rh0 < 0 ? ry0 + rh0 : ry0;
                const rw = Math.abs(rw0), rh = Math.abs(rh0), oracle = new Uint8ClampedArray(rw * rh * 4);
                for (let y = 0; y < rh; y++) for (let x = 0; x < rw; x++) {
                    if (x + rx < 0 || x + rx >= width || y + ry < 0 || y + ry >= height) continue;
                    for (let channel = 0; channel < 4; channel++)
                        oracle[(y * rw + x) * 4 + channel] = expected[((y + ry) * width + x + rx) * 4 + channel];
                }
                const result = context.getImageData(rx0, ry0, rw0, rh0);
                equal(result.data, oracle, 'read intersection and transparent padding');
                result.data.fill(201);
                equal(context.getImageData(0, 0, width, height).data, expected, 'readback owns storage');
            }
            source.data[0] ^= 127;
            equal(context.getImageData(0, 0, width, height).data, expected, 'write copied source');
            source.data[0] ^= 127;
        }
    }
    return assertions;
}
