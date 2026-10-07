// A small bitmap selects the unchanged JS clip rasterizer, while a larger
// bitmap selects the native provider. Compare common pixels without modifying
// any platform hook or depending on native admission for the oracle.
function testCanvasClipMembership(make) {
    const contours = [
        [],
        [[[1, 2], [12, 2], [12, 15], [1, 15]]],
        [[[1.5, 2.5], [11.5, 2.5], [11.5, 14.5], [1.5, 14.5]]],
        [[[0, 0], [12, 0], [12, 16], [0, 16]],
            [[3, 4], [9, 4], [9, 12], [3, 12]]],
        [[[0, 0], [12, 0], [12, 16], [0, 16]],
            [[3, 12], [9, 12], [9, 4], [3, 4]]],
        [[[1, 1], [11, 15], [1, 15], [11, 1]]],
        [[[-4.25, 3.75], [18.5, 7.5], [6.25, 21.5]]],
        [[[2.5, 2.5], [10.5, 14.5]]],
        [[[2, 2], [10, 2], [10, 14], [2, 14]],
            [[2, 2], [10, 2], [10, 14], [2, 14]]]
    ];
    const transformPoint = (point, transform) => transform ?
        [point[0] - .25 * point[1] + 2, .125 * point[0] + point[1]] : point;
    // Independent ray-crossing membership, with the same documented binary
    // center sample. Do not use getImageData from either implementation as the
    // expected value, so two identically broken providers cannot pass.
    const member = (parts, x, y, rule, transform) => {
        let winding = 0, crossings = 0;
        for (const part of parts) {
            if (part.length < 2) continue;
            for (let index = 0; index < part.length; index++) {
                const [x0, y0] = transformPoint(part[index], transform);
                const [x1, y1] = transformPoint(part[(index + 1) % part.length], transform);
                if ((y0 <= y && y1 > y) || (y1 <= y && y0 > y)) {
                    const crossing = x0 + (y - y0) * (x1 - x0) / (y1 - y0);
                    if (crossing <= x) {
                        crossings++;
                        winding += y1 > y0 ? 1 : -1;
                    }
                }
            }
        }
        return rule === 'evenodd' ? crossings % 2 !== 0 : winding !== 0;
    };
    const draw = (width, height, parts, rule, transformed, explicitPath, nested) => {
        const context = make(width, height).getContext('2d');
        if (transformed) context.setTransform(1, .125, -.25, 1, 2, 0);
        const path = explicitPath ? new Path2D() : context;
        if (!explicitPath) context.beginPath();
        for (const part of parts) {
            if (!part.length) continue;
            path.moveTo(...part[0]);
            for (const point of part.slice(1)) path.lineTo(...point);
            // Intentionally leave each contour open. clip() closes it without
            // mutating the path used by subsequent stroke/point operations.
        }
        if (explicitPath) context.clip(path, rule);
        else context.clip(rule);
        context.resetTransform();
        if (nested) {
            context.save();
            context.beginPath(); context.rect(0, 0, 0, 0); context.clip();
            context.restore();
            context.beginPath(); context.rect(2, 3, 8, 10); context.clip();
        }
        context.fillStyle = '#fc3';
        context.fillRect(0, 0, width, height);
        return context.getImageData(0, 0, width, height).data;
    };
    let cases = 0;
    for (const parts of contours) for (const rule of ['nonzero', 'evenodd'])
        for (const transformed of [false, true]) for (const explicitPath of [false, true])
            for (const nested of [false, true]) {
                const scalar = draw(13, 17, parts, rule, transformed, explicitPath, nested);
                const native = draw(43, 31, parts, rule, transformed, explicitPath, nested);
                for (let y = 0; y < 17; y++) for (let x = 0; x < 13; x++) {
                    const covered = member(parts, x + .5, y + .5, rule, transformed) &&
                        (!nested || x >= 2 && x < 10 && y >= 3 && y < 13);
                    const expected = covered ? [255, 204, 51, 255] : [0, 0, 0, 0];
                    for (let channel = 0; channel < 4; channel++) {
                        const a = scalar[(y * 13 + x) * 4 + channel];
                        const b = native[(y * 43 + x) * 4 + channel];
                        if (a !== expected[channel] || b !== expected[channel])
                            throw Error(`clip ${cases} ${x},${y}:${channel}: scalar=${a},native=${b},expected=${expected[channel]}`);
                    }
                }
                cases++;
            }
    return cases;
}
