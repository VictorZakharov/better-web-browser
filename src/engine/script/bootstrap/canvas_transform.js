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
    const transformCanvasPath = (path, matrix) => {
        if (canvasIsIdentity(matrix)) return path;
        const transformed = copyCanvasPath(path);
        for (const part of transformed.subpaths) for (const point of part.points) {
            const [x, y] = matrixPoint2D(matrix, point[0], point[1]);
            point[0] = x; point[1] = y;
        }
        return transformed;
    };
    const canvasTransformValues = args => args.length <= 1 ? matrixDictionary2D(args[0]) :
        args.length >= 6 ? args.slice(0,6).map(value=>+value) :
            (() => { throw new TypeError('Expected a matrix dictionary or six components'); })();
    CanvasRenderingContext2D.prototype.getTransform = function() {
        canvasImageDataContext(this);
        return new DOMMatrix(this.__transform);
    };
    CanvasRenderingContext2D.prototype.resetTransform = function() {
        canvasImageDataContext(this);
        this.__transform = identity2D();
    };
    CanvasRenderingContext2D.prototype.setTransform = function(...args) {
        canvasImageDataContext(this);
        const values = canvasTransformValues(args);
        if (values.every(Number.isFinite)) this.__transform = values;
    };
    CanvasRenderingContext2D.prototype.transform = function(a,b,c,d,e,f) {
        canvasImageDataContext(this);
        if (arguments.length < 6) throw new TypeError('transform requires six components');
        const values = [a,b,c,d,e,f].map(value=>+value);
        if (values.every(Number.isFinite))
            this.__transform = matrixMultiply2D(this.__transform, values);
    };
    CanvasRenderingContext2D.prototype.translate = function(x, y) {
        canvasImageDataContext(this);
        if(arguments.length<2)throw new TypeError('translate requires two arguments');
        const values=[+x,+y];
        if(values.every(Number.isFinite))this.__transform=matrixMultiply2D(this.__transform,[1,0,0,1,...values]);
    };
    CanvasRenderingContext2D.prototype.scale = function(x, y) {
        canvasImageDataContext(this);
        if(arguments.length<2)throw new TypeError('scale requires two arguments');
        const values=[+x,+y];
        if(values.every(Number.isFinite))this.__transform=matrixMultiply2D(this.__transform,[values[0],0,0,values[1],0,0]);
    };
    CanvasRenderingContext2D.prototype.rotate = function(angle) {
        canvasImageDataContext(this);
        if(arguments.length<1)throw new TypeError('rotate requires an argument');
        angle = +angle;
        if (!Number.isFinite(angle)) return;
        const cosine = Math.cos(angle), sine = Math.sin(angle);
        this.__transform=matrixMultiply2D(this.__transform,[cosine,sine,-sine,cosine,0,0]);
    };
