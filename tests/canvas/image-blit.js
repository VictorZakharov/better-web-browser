'use strict';
function testCanvasImageBlits(make) {
    const equal = (actual, expected, label) => {
        if (actual.length !== expected.length) throw Error(label + ': length');
        for (let i = 0; i < actual.length; i++)
            if (actual[i] !== expected[i]) throw Error(label + ': byte ' + i +
                ' (' + actual[i] + ' != ' + expected[i] + ')');
    };
    let cases = 0;
    for (const opaque of [false, true]) {
        const source = make(32, 24), sourceContext = source.getContext('2d');
        const pixels = new ImageData(32, 24);
        for (let i = 0; i < pixels.data.length / 4; i++) {
            pixels.data.set([(i * 17) % 256, (i * 31) % 256, (i * 7) % 256,
                opaque ? 255 : [0, 1, 64, 128, 254, 255][i % 6]], i * 4);
        }
        sourceContext.putImageData(pixels, 0, 0);
        const unchanged = sourceContext.getImageData(0, 0, 32, 24).data;
        for (const smooth of [false, true]) for (const clipped of [false, true]) {
            for (const [tx, ty] of [[0, 0], [5, -3], [-7, 9]]) {
                for (const [sx, sy, sw, sh, dx, dy] of [
                    [0, 0, 32, 24, 11, 7], [-4, -3, 40, 32, 6, 5],
                    [6, 5, 24, 16, -2, -3]
                ]) {
                    const draw = tiled => {
                        const canvas = make(56, 44), c = canvas.getContext('2d');
                        c.fillStyle = 'rgba(71,149,203,.5)'; c.fillRect(0, 0, 56, 44);
                        if (clipped) {
                            c.beginPath(); c.rect(3, 4, 19, 29); c.rect(27, 11, 25, 19); c.clip();
                        }
                        c.translate(tx, ty); c.imageSmoothingEnabled = smooth;
                        if (!tiled) c.drawImage(source, sx, sy, sw, sh, dx, dy, sw, sh);
                        else for (let y = 0; y < sh; y += 8) for (let x = 0; x < sw; x += 8) {
                            const width = Math.min(8, sw - x), height = Math.min(8, sh - y);
                            // Small draws retain the JavaScript sampling path, not the
                            // native row-copy path. Every tile reads the original source.
                            c.drawImage(source, sx + x, sy + y, width, height,
                                dx + x, dy + y, width, height);
                        }
                        return c.getImageData(0, 0, 56, 44).data;
                    };
                    equal(draw(false), draw(true), 'translated/cropped/clipped blit');
                    cases++;
                }
            }
        }
        equal(sourceContext.getImageData(0, 0, 32, 24).data, unchanged, 'source ownership');
    }
    // Native self-drawing must snapshot before any overlapping row is written.
    const self = make(64, 32), c = self.getContext('2d'), original = new ImageData(64, 32);
    for (let y = 0; y < 32; y++) for (let x = 0; x < 64; x++)
        original.data.set([x * 3, y * 7, 91, 255], (y * 64 + x) * 4);
    c.putImageData(original, 0, 0); c.drawImage(self, 0, 0, 40, 32, 10, 0, 40, 32);
    const expected = new Uint8ClampedArray(original.data);
    for (let y = 0; y < 32; y++) for (let x = 10; x < 50; x++)
        expected.set(original.data.subarray((y * 64 + x - 10) * 4,
            (y * 64 + x - 10) * 4 + 4), (y * 64 + x) * 4);
    equal(c.getImageData(0, 0, 64, 32).data, expected, 'overlapping source snapshot');
    return cases + 1;
}
