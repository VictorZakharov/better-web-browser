    // Canvas 2D uses post-multiplied matrices; the current path stores points
    // in bitmap coordinates at construction time, unlike a supplied Path2D.
    const canvasIsIdentity = matrix => matrix.every((value, index) =>
        value === identity2D()[index]);
    const canvasTransformedBounds = (matrix, x, y, width, height, state) => {
        const points = [[x, y], [x + width, y], [x, y + height], [x + width, y + height]]
            .map(([px, py]) => matrixPoint2D(matrix, px, py));
        if (points.some(point => !point.every(Number.isFinite))) return null;
        const xs = points.map(point => point[0]), ys = points.map(point => point[1]);
        return [Math.max(0, Math.floor(Math.min(...xs))), Math.max(0, Math.floor(Math.min(...ys))),
            Math.min(state.width, Math.ceil(Math.max(...xs))),
            Math.min(state.height, Math.ceil(Math.max(...ys)))];
    };
    const paintTransformedCanvasRect = (context, state, rect, style) => {
        const inverse = matrixInverse2D(context.__transform);
        const bounds = canvasTransformedBounds(context.__transform,
            rect.x, rect.y, rect.width, rect.height, state);
        if (!inverse || !bounds) return;
        const [left, top, right, bottom] = bounds;
        for (let row = top; row < bottom; row++) for (let column = left; column < right; column++) {
            if (!canvasClipAllows(context, column, row, state.width)) continue;
            const [x, y] = matrixPoint2D(inverse, column + 0.5, row + 0.5);
            if (x < rect.x || y < rect.y || x >= rect.x + rect.width || y >= rect.y + rect.height)
                continue;
            const offset = (row * state.width + column) * 4;
            if (!style) state.pixels.fill(0, offset, offset + 4);
            else compositeCanvasPixel(state.pixels, offset,
                canvasPaintAt(style, column + 0.5, row + 0.5, inverse),
                context.__globalAlpha, context.__compositeOperation);
        }
    };
    const transformCanvasPath = (path, matrix) => {
        if (canvasIsIdentity(matrix)) return path;
        const transformed = copyCanvasPath(path);
        for (const part of transformed.subpaths) for (const point of part.points) {
            const [x, y] = matrixPoint2D(matrix, point[0], point[1]);
            point[0] = x; point[1] = y;
        }
        return transformed;
    };
    const canvasTransformValues = args => args.length === 1 ? matrixComponents2D(args[0]) :
        args.length === 6 ? args.map(Number) : (() => { throw new TypeError('Expected a matrix or six components'); })();
    CanvasRenderingContext2D.prototype.getTransform = function() {
        return new DOMMatrix(this.__transform);
    };
    CanvasRenderingContext2D.prototype.resetTransform = function() {
        this.__transform = identity2D();
    };
    CanvasRenderingContext2D.prototype.setTransform = function(...args) {
        const values = args.length ? canvasTransformValues(args) : identity2D();
        if (values.every(Number.isFinite)) this.__transform = values;
    };
    CanvasRenderingContext2D.prototype.transform = function(...args) {
        if (args.length !== 6) throw new TypeError('transform requires six components');
        const values = args.map(Number);
        if (values.every(Number.isFinite))
            this.__transform = matrixMultiply2D(this.__transform, values);
    };
    CanvasRenderingContext2D.prototype.translate = function(x, y) {
        this.transform(1, 0, 0, 1, x, y);
    };
    CanvasRenderingContext2D.prototype.scale = function(x, y = x) {
        this.transform(x, 0, 0, y, 0, 0);
    };
    CanvasRenderingContext2D.prototype.rotate = function(angle) {
        angle = Number(angle);
        if (!Number.isFinite(angle)) return;
        const cosine = Math.cos(angle), sine = Math.sin(angle);
        this.transform(cosine, sine, -sine, cosine, 0, 0);
    };
