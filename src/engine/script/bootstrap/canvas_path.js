    // A path is a list of bounded, flattened subpaths. Curves are subdivided before rasterization;
    // the original Canvas current path is deliberately not part of the save/restore drawing state.
    const MAX_CANVAS_PATH_POINTS = 8192;
    const canvasCurveHost = __hostCall;
    const canvasCurveStringify = canvasPrivateWireStringify;
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
        moveCanvasPath(path, x, y);
    };
    const ellipseCanvasPath = (path, x, y, radiusX, radiusY, rotation, startAngle, endAngle,
        counterclockwise = false, transform = null) => {
        [x, y, radiusX, radiusY, rotation, startAngle, endAngle] =
            [x, y, radiusX, radiusY, rotation, startAngle, endAngle].map(Number);
        if (!canvasPoint([x, y, radiusX, radiusY, rotation, startAngle, endAngle])) return;
        if (radiusX < 0 || radiusY < 0) throw new DOMException('Ellipse radii must be non-negative', 'IndexSizeError');
        const tau = 2 * canvasPrivateMath.PI;
        let sweep = endAngle - startAngle;
        if (!counterclockwise && sweep >= tau) sweep = tau;
        else if (counterclockwise && -sweep >= tau) sweep = -tau;
        else {
            // Modulo normalization cannot spin forever for very large finite
            // angles whose subtraction overflows or cannot change by one turn.
            sweep = ((endAngle % tau - startAngle % tau) % tau + tau) % tau;
            if (counterclockwise && sweep !== 0) sweep -= tau;
        }
        startAngle %= tau; rotation %= tau;
        const native = canvasCurveHost('canvasCurvePoints', canvasCurveStringify({kind:'arc', arc:{
            center:[x,y], radii:[radiusX,radiusY], start:startAngle, sweep, rotation,
            transform:transform || [1,0,0,1,0,0]
        }}));
        if (native) {
            const reopen = path.current !== null && path.subpaths[path.current].closed ? 1 : 0;
            if (path.pointCount + native.length + reopen > MAX_CANVAS_PATH_POINTS)
                throw new DOMException('Canvas path exceeds the geometry budget', 'NotSupportedError');
            for (let index=0;index<native.length;index++) {
                if (index === 0 && path.current === null) moveCanvasPath(path, ...native[index]);
                else lineCanvasPath(path, ...native[index]);
            }
            return;
        }
        const steps = canvasPrivateMath.max(1, canvasPrivateMath.min(256, canvasPrivateMath.ceil(canvasPrivateMath.abs(sweep) * canvasPrivateMath.max(radiusX, radiusY) / 2)));
        const cosine = canvasPrivateMath.cos(rotation), sine = canvasPrivateMath.sin(rotation);
        for (let step = 0; step <= steps; step++) {
            const angle = startAngle + sweep * step / steps;
            const localX = radiusX * canvasPrivateMath.cos(angle), localY = radiusY * canvasPrivateMath.sin(angle);
            const pointX = x + localX * cosine - localY * sine;
            const pointY = y + localX * sine + localY * cosine;
            const [paintX, paintY] = transform ? matrixPoint2D(transform, pointX, pointY) : [pointX, pointY];
            if (step === 0 && path.current === null) moveCanvasPath(path, paintX, paintY);
            else lineCanvasPath(path, paintX, paintY);
        }
    };
    const curveCanvasPath = (path, controls, cubic) => {
        const values = controls.map(Number);
        if (!canvasPoint(values)) return;
        // An empty path starts at the first control point, not (0,0). Closing
        // a contour leaves its first point as the current point for the next one.
        if (path.current === null) moveCanvasPath(path, values[0], values[1]);
        else if (path.subpaths[path.current].closed)
            moveCanvasPath(path, ...path.subpaths[path.current].points[0]);
        const points = path.subpaths[path.current].points;
        const [x0, y0] = points[points.length - 1];
        const native = canvasCurveHost('canvasCurvePoints', canvasCurveStringify({
            kind:cubic?'cubic':'quadratic', points:[[x0,y0],...Array.from({length:values.length/2},
                (_,index)=>values.slice(index*2,index*2+2))]
        }));
        if (native) {
            if (path.pointCount + native.length - 1 > MAX_CANVAS_PATH_POINTS)
                throw new DOMException('Canvas path exceeds the geometry budget', 'NotSupportedError');
            for (let index=1;index<native.length;index++) lineCanvasPath(path, ...native[index]);
            return;
        }
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
    const installCanvasPathMethods = (prototype, pathFor, transformFor = () => null) => {
        const point = (context, x, y) => transformFor(context) ?
            matrixPoint2D(transformFor(context), Number(x), Number(y)) : [x, y];
        prototype.moveTo = function(x, y) { moveCanvasPath(pathFor(this), ...point(this, x, y)); };
        prototype.lineTo = function(x, y) { lineCanvasPath(pathFor(this), ...point(this, x, y)); };
        prototype.closePath = function() { closeCanvasPath(pathFor(this)); };
        prototype.rect = function(x, y, width, height) {
            const path = pathFor(this), transform = transformFor(this);
            if (!transform) { rectCanvasPath(path, x, y, width, height); return; }
            [x, y, width, height] = [x, y, width, height].map(Number);
            if (!canvasPoint([x, y, width, height])) return;
            const corners = [[x, y], [x + width, y], [x + width, y + height], [x, y + height]]
                .map(([px, py]) => matrixPoint2D(transform, px, py));
            moveCanvasPath(path, ...corners[0]);
            for (const corner of corners.slice(1)) lineCanvasPath(path, ...corner);
            closeCanvasPath(path);
            moveCanvasPath(path, ...corners[0]);
        };
        prototype.arc = function(x, y, radius, start, end, counterclockwise = false) {
            ellipseCanvasPath(pathFor(this), x, y, radius, radius, 0, start, end,
                counterclockwise, transformFor(this));
        };
        prototype.ellipse = function(x, y, rx, ry, rotation, start, end, counterclockwise = false) {
            ellipseCanvasPath(pathFor(this), x, y, rx, ry, rotation, start, end,
                counterclockwise, transformFor(this));
        };
        prototype.quadraticCurveTo = function(cpx, cpy, x, y) {
            const controls = [...point(this, cpx, cpy), ...point(this, x, y)];
            curveCanvasPath(pathFor(this), controls, false);
        };
        prototype.bezierCurveTo = function(cp1x, cp1y, cp2x, cp2y, x, y) {
            const controls = [...point(this, cp1x, cp1y), ...point(this, cp2x, cp2y),
                ...point(this, x, y)];
            curveCanvasPath(pathFor(this), controls, true);
        };
        bindCanvasPathNumbers(prototype, [['moveTo',2], ['lineTo',2], ['closePath',0],
            ['rect',4], ['arc',5,'boolean'], ['ellipse',7,'boolean'],
            ['quadraticCurveTo',4], ['bezierCurveTo',6]]);
    };
    class Path2D {
        constructor(source) {
            if (canvasPathData.has(source)) canvasPathData.set(this, copyCanvasPath(canvasPathData.get(source)));
            else if (source === undefined) canvasPathData.set(this, newCanvasPath());
            else canvasPathData.set(this, parseCanvasSvgPath(`${source}`));
        }
        addPath(path, transform = {}) {
            if(!canvasPathData.has(this))throw new TypeError('addPath requires a Path2D receiver');
            if (arguments.length<1||!canvasPathData.has(path)) throw new TypeError('addPath requires Path2D');
            const matrix=matrixDictionary2D(transform);
            if(!canvasPoint(matrix))return;
            const target = canvasPathData.get(this);
            const addition = copyCanvasPath(canvasPathData.get(path));
            if (target.pointCount + addition.pointCount > MAX_CANVAS_PATH_POINTS)
                throw new DOMException('Canvas path exceeds the geometry budget', 'NotSupportedError');
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
    Object.defineProperty(Path2D.prototype,'addPath',{enumerable:true});
    installCanvasPathMethods(Path2D.prototype, path => canvasPathData.get(path));
