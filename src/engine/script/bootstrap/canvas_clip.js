    // Clip regions are immutable packed bitmaps. save()/restore() can share a
    // region without copying up to four million bytes per drawing state.
    // https://html.spec.whatwg.org/multipage/canvas.html#dom-context-2d-clip
    const canvasClipAllows = (context, x, y, width) => {
        const bits = context.__clipBits;
        if (!bits) return true;
        const index = y * width + x;
        return (bits[index >> 3] & (1 << (index & 7))) !== 0;
    };
    const canvasSetClipBit = (bits, index) => {
        bits[index >> 3] |= 1 << (index & 7);
    };
    const canvasRasterClip = (path, rule, state, previous) => {
        const bits = new Uint8Array(Math.ceil(state.width * state.height / 8));
        const edges = pathEdges(path);
        const [left, top, right, bottom] = canvasPixelBounds(path, state, 0);
        const work = Math.max(0, right - left) * Math.max(0, bottom - top) * edges.length;
        if (work > 50000000)
            throw new DOMException('Canvas clip exceeds the raster budget', 'NotSupportedError');
        for (let y = top; y < bottom; y++) {
            const intersections = [];
            const sampleY = y + 0.5;
            for (const [[x0, y0], [x1, y1]] of edges) {
                if ((y0 <= sampleY && y1 > sampleY) || (y1 <= sampleY && y0 > sampleY))
                    intersections.push([x0 + (sampleY - y0) * (x1 - x0) / (y1 - y0),
                        y1 > y0 ? 1 : -1]);
            }
            intersections.sort((a, b) => a[0] - b[0]);
            let edge = 0, crossings = 0, winding = 0;
            for (let x = left; x < right; x++) {
                while (edge < intersections.length && intersections[edge][0] <= x + 0.5) {
                    crossings++; winding += intersections[edge++][1];
                }
                const covered = rule === 'evenodd' ? crossings % 2 !== 0 : winding !== 0;
                const index = y * state.width + x;
                if (covered && (!previous || (previous[index >> 3] & (1 << (index & 7)))))
                    canvasSetClipBit(bits, index);
            }
        }
        return bits;
    };
    CanvasRenderingContext2D.prototype.clip = function(pathOrRule, rule) {
        const path = canvasPathArgument(this, pathOrRule);
        const fillRule = canvasFillRule(pathOrRule instanceof Path2D ? rule : pathOrRule);
        const state = stateForCanvas(this.canvas);
        if (!state.pixels) return;
        this.__clipBits = canvasRasterClip(path, fillRule, state, this.__clipBits);
    };
