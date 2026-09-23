    // A path is a list of bounded, flattened subpaths. Curves are subdivided before rasterization;
    // the original Canvas current path is deliberately not part of the save/restore drawing state.
    const MAX_CANVAS_PATH_POINTS = 8192;
    const canvasPathData = new WeakMap();
    const newCanvasPath = () => ({ subpaths: [], current: null, pointCount: 0 });
    const copyCanvasPath = path => ({
        subpaths: path.subpaths.map(part => ({ points: part.points.map(point => [...point]), closed: part.closed })),
        current: path.current, pointCount: path.pointCount
    });
    const reserveCanvasPoint = path => {
        if (path.pointCount >= MAX_CANVAS_PATH_POINTS)
            throw new DOMException('Canvas path exceeds the geometry budget', 'NotSupportedError');
        path.pointCount++;
    };
    const canvasPoint = values => values.every(Number.isFinite);
    const moveCanvasPath = (path, x, y) => {
        x = Number(x); y = Number(y);
        if (!canvasPoint([x, y])) return;
        reserveCanvasPoint(path);
        path.subpaths.push({ points: [[x, y]], closed: false });
        path.current = path.subpaths.length - 1;
    };
    const lineCanvasPath = (path, x, y) => {
        x = Number(x); y = Number(y);
        if (!canvasPoint([x, y])) return;
        if (path.current === null) { moveCanvasPath(path, x, y); return; }
        if (path.subpaths[path.current].closed) {
            const [startX, startY] = path.subpaths[path.current].points[0];
            moveCanvasPath(path, startX, startY);
        }
        reserveCanvasPoint(path);
        path.subpaths[path.current].points.push([x, y]);
    };
    const closeCanvasPath = path => {
        if (path.current !== null) path.subpaths[path.current].closed = true;
    };
    const rectCanvasPath = (path, x, y, width, height) => {
        [x, y, width, height] = [x, y, width, height].map(Number);
        if (!canvasPoint([x, y, width, height])) return;
        moveCanvasPath(path, x, y);
        lineCanvasPath(path, x + width, y);
        lineCanvasPath(path, x + width, y + height);
        lineCanvasPath(path, x, y + height);
        closeCanvasPath(path);
    };
    const ellipseCanvasPath = (path, x, y, radiusX, radiusY, rotation, startAngle, endAngle,
        counterclockwise = false) => {
        [x, y, radiusX, radiusY, rotation, startAngle, endAngle] =
            [x, y, radiusX, radiusY, rotation, startAngle, endAngle].map(Number);
        if (!canvasPoint([x, y, radiusX, radiusY, rotation, startAngle, endAngle])) return;
        if (radiusX < 0 || radiusY < 0) throw new DOMException('Ellipse radii must be non-negative', 'IndexSizeError');
        const tau = 2 * Math.PI;
        let sweep = endAngle - startAngle;
        if (!counterclockwise && sweep >= tau) sweep = tau;
        else if (counterclockwise && -sweep >= tau) sweep = -tau;
        else if (!counterclockwise) { while (sweep < 0) sweep += tau; }
        else { while (sweep > 0) sweep -= tau; }
        const steps = Math.max(1, Math.min(256, Math.ceil(Math.abs(sweep) * Math.max(radiusX, radiusY) / 2)));
        const cosine = Math.cos(rotation), sine = Math.sin(rotation);
        for (let step = 0; step <= steps; step++) {
            const angle = startAngle + sweep * step / steps;
            const localX = radiusX * Math.cos(angle), localY = radiusY * Math.sin(angle);
            const pointX = x + localX * cosine - localY * sine;
            const pointY = y + localX * sine + localY * cosine;
            if (step === 0 && path.current === null) moveCanvasPath(path, pointX, pointY);
            else lineCanvasPath(path, pointX, pointY);
        }
    };
    const curveCanvasPath = (path, controls, cubic) => {
        const values = controls.map(Number);
        if (!canvasPoint(values)) return;
        if (path.current === null) moveCanvasPath(path, 0, 0);
        const points = path.subpaths[path.current].points;
        const [x0, y0] = points[points.length - 1];
        const steps = 24;
        for (let step = 1; step <= steps; step++) {
            const t = step / steps, u = 1 - t;
            const x = cubic ? u*u*u*x0 + 3*u*u*t*values[0] + 3*u*t*t*values[2] + t*t*t*values[4] :
                u*u*x0 + 2*u*t*values[0] + t*t*values[2];
            const y = cubic ? u*u*u*y0 + 3*u*u*t*values[1] + 3*u*t*t*values[3] + t*t*t*values[5] :
                u*u*y0 + 2*u*t*values[1] + t*t*values[3];
            lineCanvasPath(path, x, y);
        }
    };
    const installCanvasPathMethods = (prototype, pathFor) => {
        prototype.moveTo = function(x, y) { moveCanvasPath(pathFor(this), x, y); };
        prototype.lineTo = function(x, y) { lineCanvasPath(pathFor(this), x, y); };
        prototype.closePath = function() { closeCanvasPath(pathFor(this)); };
        prototype.rect = function(x, y, width, height) { rectCanvasPath(pathFor(this), x, y, width, height); };
        prototype.arc = function(x, y, radius, start, end, counterclockwise = false) {
            ellipseCanvasPath(pathFor(this), x, y, radius, radius, 0, start, end, counterclockwise);
        };
        prototype.ellipse = function(x, y, rx, ry, rotation, start, end, counterclockwise = false) {
            ellipseCanvasPath(pathFor(this), x, y, rx, ry, rotation, start, end, counterclockwise);
        };
        prototype.quadraticCurveTo = function(cpx, cpy, x, y) {
            curveCanvasPath(pathFor(this), [cpx, cpy, x, y], false);
        };
        prototype.bezierCurveTo = function(cp1x, cp1y, cp2x, cp2y, x, y) {
            curveCanvasPath(pathFor(this), [cp1x, cp1y, cp2x, cp2y, x, y], true);
        };
    };
    class Path2D {
        constructor(source) {
            if (source instanceof Path2D) canvasPathData.set(this, copyCanvasPath(canvasPathData.get(source)));
            else if (source === undefined) canvasPathData.set(this, newCanvasPath());
            else if (typeof source === 'string') canvasPathData.set(this, parseCanvasSvgPath(source));
            else throw new TypeError('Path2D requires a path or SVG path string');
        }
        addPath(path, transform = {}) {
            if (!(path instanceof Path2D)) throw new TypeError('addPath requires Path2D');
            const target = canvasPathData.get(this);
            const addition = copyCanvasPath(canvasPathData.get(path));
            if (target.pointCount + addition.pointCount > MAX_CANVAS_PATH_POINTS)
                throw new DOMException('Canvas path exceeds the geometry budget', 'NotSupportedError');
            const { a = 1, b = 0, c = 0, d = 1, e = 0, f = 0 } = transform;
            const matrix = [a, b, c, d, e, f].map(Number);
            if (!canvasPoint(matrix)) throw new TypeError('Path transform components must be finite');
            for (const part of addition.subpaths) for (const point of part.points) {
                const [x, y] = point;
                point[0] = matrix[0] * x + matrix[2] * y + matrix[4];
                point[1] = matrix[1] * x + matrix[3] * y + matrix[5];
            }
            target.subpaths.push(...addition.subpaths);
            target.pointCount += addition.pointCount;
            target.current = target.subpaths.length ? target.subpaths.length - 1 : null;
        }
    }
    installCanvasPathMethods(Path2D.prototype, path => canvasPathData.get(path));
    // A deliberately strict SVG subset: unsupported commands reject instead of drawing a misleading shape.
    const parseCanvasSvgPath = source => {
        if (source.length > 131072)
            throw new DOMException('SVG path data exceeds the geometry budget', 'NotSupportedError');
        const path = newCanvasPath();
        const tokens = source.match(/[MmLlHhVvZz]|[-+]?(?:\d*\.\d+|\d+\.?\d*)(?:[eE][-+]?\d+)?|[^\s,]/g) || [];
        let index = 0, command = '', x = 0, y = 0, startX = 0, startY = 0;
        while (index < tokens.length) {
            if (/^[A-Za-z]$/.test(tokens[index])) command = tokens[index++];
            if (!/[MmLlHhVvZz]/.test(command)) throw new DOMException('Unsupported SVG path command', 'NotSupportedError');
            if (/[Zz]/.test(command)) {
                closeCanvasPath(path); x = startX; y = startY; command = '';
                continue;
            }
            const count = /[HhVv]/.test(command) ? 1 : 2;
            if (index + count > tokens.length || tokens.slice(index, index + count).some(token => !Number.isFinite(Number(token)) || /^[A-Za-z]$/.test(token)))
                throw new DOMException('Invalid SVG path data', 'SyntaxError');
            const values = tokens.slice(index, index + count).map(Number); index += count;
            const relative = command === command.toLowerCase();
            if (/[Hh]/.test(command)) x = relative ? x + values[0] : values[0];
            else if (/[Vv]/.test(command)) y = relative ? y + values[0] : values[0];
            else { x = relative ? x + values[0] : values[0]; y = relative ? y + values[1] : values[1]; }
            if (/[Mm]/.test(command)) {
                moveCanvasPath(path, x, y); startX = x; startY = y;
                command = relative ? 'l' : 'L';
            } else lineCanvasPath(path, x, y);
        }
        return path;
    };
