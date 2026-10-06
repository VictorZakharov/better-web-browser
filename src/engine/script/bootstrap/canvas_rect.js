    // Rectangles retain double coordinates and do not modify the current path.
    // https://html.spec.whatwg.org/multipage/canvas.html#drawing-rectangles-to-the-bitmap
    // Integrate the transformed convex quadrilateral over each pixel square.
    // Axis-aligned rectangles use interval overlap, including reflected matrices.
    const canvasClipPolygonAxis = (points, axis, boundary, greater) => {
        const output = [];
        if (!points.length) return output;
        let previous = points[points.length - 1];
        let previousInside = greater ? previous[axis] >= boundary : previous[axis] <= boundary;
        for (const current of points) {
            const inside = greater ? current[axis] >= boundary : current[axis] <= boundary;
            if (inside !== previousInside) {
                const fraction = (boundary - previous[axis]) / (current[axis] - previous[axis]);
                const point = [previous[0] + fraction * (current[0] - previous[0]),
                    previous[1] + fraction * (current[1] - previous[1])];
                point[axis] = boundary;
                output.push(point);
            }
            if (inside) output.push(current);
            previous = current; previousInside = inside;
        }
        return output;
    };
    const canvasRectPixelArea = (polygon, x, y) => {
        let points = canvasClipPolygonAxis(polygon, 0, x, true);
        points = canvasClipPolygonAxis(points, 0, x + 1, false);
        points = canvasClipPolygonAxis(points, 1, y, true);
        points = canvasClipPolygonAxis(points, 1, y + 1, false);
        // Subtract the pixel origin before computing area: large absolute
        // coordinates must not cancel away a subpixel intersection.
        let twiceArea = 0;
        for (let index = 0; index < points.length; index++) {
            const a = points[index], b = points[(index + 1) % points.length];
            twiceArea += (a[0] - x) * (b[1] - y) - (a[1] - y) * (b[0] - x);
        }
        return Math.min(1, Math.abs(twiceArea) / 2);
    };
    const canvasRectSolidRows = (context, state, style, left, top, right, bottom) => {
        if (canvasDrawingState(context).clipBits || (style && (canvasDrawingState(context).compositeOperation !== 'source-over' ||
            canvasDrawingState(context).globalAlpha !== 1 || !style.channels || style.channels[3] !== 255))) return false;
        const stride = state.width * 4, length = (right - left) * 4;
        if (length <= 0 || top >= bottom) return true;
        if (!style) {
            for (let y = top; y < bottom; y++)
                canvasClearBitmapRange(state.pixels, y * stride + left * 4, y * stride + right * 4);
        } else {
            // A bounded reusable row replaces per-pixel paint/compositing calls.
            // Use captured typed-array intrinsics; no author callback can run
            // while copying a row, even if set/fill are replaced on prototypes.
            const row = new canvasPixelArray(length), color = style.channels;
            for (let x = 0; x < length; x += 4) {
                row[x] = color[0]; row[x + 1] = color[1]; row[x + 2] = color[2]; row[x + 3] = 255;
            }
            for (let y = top; y < bottom; y++) canvasPixelSet(state.pixels, row, y * stride + left * 4);
        }
        return true;
    };
    const paintTransformedCanvasRect = (context, state, rect, style) => {
        const matrix = canvasDrawingState(context).transform, inverse = matrixInverse2D(matrix);
        if (!inverse) return;
        const polygon = [[rect.x, rect.y], [rect.x + rect.width, rect.y],
            [rect.x + rect.width, rect.y + rect.height], [rect.x, rect.y + rect.height]]
            .map(([x, y]) => matrixPoint2D(matrix, x, y));
        if (polygon.some(point => !point.every(Number.isFinite))) return;
        const xs = polygon.map(point => point[0]), ys = polygon.map(point => point[1]);
        const minX = Math.min(...xs), maxX = Math.max(...xs);
        const minY = Math.min(...ys), maxY = Math.max(...ys);
        const aligned = (matrix[1] === 0 && matrix[2] === 0) || (matrix[0] === 0 && matrix[3] === 0);
        // clearRect erases whole pixels, unlike antialiased painting. Match the
        // reference rasterizer's nearest-edge interval for axis-aligned clears.
        const roundClear = !style && aligned;
        const low = value => roundClear ? Math.floor(value + .5) : Math.floor(value);
        const high = value => roundClear ? Math.floor(value + .5) : Math.ceil(value);
        const left = Math.max(0, low(minX)), top = Math.max(0, low(minY));
        const right = Math.min(state.width, high(maxX)), bottom = Math.min(state.height, high(maxY));
        if (roundClear && canvasRectSolidRows(context, state, null, left, top, right, bottom)) return;
        if (aligned && [minX, maxX, minY, maxY].every(Number.isInteger) &&
            canvasRectSolidRows(context, state, style, left, top, right, bottom)) return;
        if (aligned && [minX, maxX, minY, maxY].every(Number.isInteger) && style &&
            canvasPaintSolidMask(context, state, null, style, left, top, right, bottom)) return;
        for (let row = top; row < bottom; row++) for (let column = left; column < right; column++) {
            if (!canvasClipAllows(context, column, row, state.width)) continue;
            if (!style) {
                const [x, y] = matrixPoint2D(inverse, column + .5, row + .5);
                if (aligned || (x >= rect.x && x < rect.x + rect.width &&
                    y >= rect.y && y < rect.y + rect.height))
                    canvasClearBitmapRange(state.pixels, (row * state.width + column) * 4,
                        (row * state.width + column) * 4 + 4);
                continue;
            }
            const coverage = aligned ?
                Math.max(0, Math.min(column + 1, maxX) - Math.max(column, minX)) *
                Math.max(0, Math.min(row + 1, maxY) - Math.max(row, minY)) :
                canvasRectPixelArea(polygon, column, row);
            if (!(coverage > 0)) continue;
            const offset = (row * state.width + column) * 4;
            compositeCanvasPixel(state.pixels, offset,
                canvasPaintAt(style, column + 0.5, row + 0.5, inverse),
                canvasDrawingState(context).globalAlpha * coverage, canvasDrawingState(context).compositeOperation);
        }
    };
