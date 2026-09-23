    const canvasRadiusPoint = radius => {
        const scalar = radius === null || typeof radius !== 'object';
        const x = Number(scalar ? radius : radius.x ?? 0);
        const y = Number(scalar ? radius : radius.y ?? 0);
        if (!canvasPoint([x, y])) return null;
        if (x < 0 || y < 0) throw new RangeError('Rounded rectangle radii must be non-negative');
        return {x, y};
    };
    const canvasRoundRectRadii = (radii, width, height) => {
        const sequence = Array.isArray(radii) ? radii : [radii];
        if (sequence.length < 1 || sequence.length > 4)
            throw new RangeError('Rounded rectangle requires one to four radii');
        const points = sequence.map(canvasRadiusPoint);
        if (points.some(point => !point)) return null;
        let corners = sequence.length === 1 ? [points[0], points[0], points[0], points[0]] :
            sequence.length === 2 ? [points[0], points[1], points[0], points[1]] :
            sequence.length === 3 ? [points[0], points[1], points[2], points[1]] : points;
        corners = corners.map(point => ({...point}));
        if (width < 0) [corners[0], corners[1], corners[2], corners[3]] =
            [corners[1], corners[0], corners[3], corners[2]];
        if (height < 0) [corners[0], corners[1], corners[2], corners[3]] =
            [corners[3], corners[2], corners[1], corners[0]];
        const [ul, ur, lr, ll] = corners;
        const horizontal = Math.abs(width), vertical = Math.abs(height);
        const ratios = [
            (ul.x + ur.x) ? horizontal / (ul.x + ur.x) : Infinity,
            (ur.y + lr.y) ? vertical / (ur.y + lr.y) : Infinity,
            (lr.x + ll.x) ? horizontal / (lr.x + ll.x) : Infinity,
            (ll.y + ul.y) ? vertical / (ll.y + ul.y) : Infinity
        ];
        const scale = Math.min(1, ...ratios);
        for (const point of corners) { point.x *= scale; point.y *= scale; }
        return corners;
    };
    const installCanvasRoundedPathMethods = (prototype, pathFor, transformFor = () => null) => {
        prototype.arcTo = function(x1, y1, x2, y2, radius) {
            [x1, y1, x2, y2, radius] = [x1, y1, x2, y2, radius].map(Number);
            if (!canvasPoint([x1, y1, x2, y2, radius])) return;
            if (radius < 0) throw new DOMException('Arc radius must be non-negative', 'IndexSizeError');
            const path = pathFor(this), transform = transformFor(this);
            const paint = (x, y) => transform ? matrixPoint2D(transform, x, y) : [x, y];
            if (path.current === null) { moveCanvasPath(path, ...paint(x1, y1)); return; }
            const points = path.subpaths[path.current].points;
            const previous = points[points.length - 1];
            const inverse = transform ? matrixInverse2D(transform) : null;
            if (transform && !inverse) return;
            const [x0, y0] = inverse ? matrixPoint2D(inverse, ...previous) : previous;
            const firstLength = Math.hypot(x0 - x1, y0 - y1);
            const secondLength = Math.hypot(x2 - x1, y2 - y1);
            if (!firstLength || !secondLength || radius === 0) {
                lineCanvasPath(path, ...paint(x1, y1)); return;
            }
            const first = [(x0 - x1) / firstLength, (y0 - y1) / firstLength];
            const second = [(x2 - x1) / secondLength, (y2 - y1) / secondLength];
            const cross = first[0] * second[1] - first[1] * second[0];
            const dot = first[0] * second[0] + first[1] * second[1];
            if (Math.abs(cross) < 1e-12 || dot <= -1) {
                lineCanvasPath(path, ...paint(x1, y1)); return;
            }
            const distance = radius / Math.tan(Math.acos(Math.max(-1, Math.min(1, dot))) / 2);
            const tangent0 = [x1 + first[0] * distance, y1 + first[1] * distance];
            const tangent1 = [x1 + second[0] * distance, y1 + second[1] * distance];
            const direction = cross < 0 ? -1 : 1;
            const center = [tangent0[0] + direction * -first[1] * radius,
                tangent0[1] + direction * first[0] * radius];
            const start = Math.atan2(tangent0[1] - center[1], tangent0[0] - center[0]);
            const end = Math.atan2(tangent1[1] - center[1], tangent1[0] - center[0]);
            lineCanvasPath(path, ...paint(...tangent0));
            ellipseCanvasPath(path, ...center, radius, radius, 0, start, end, cross > 0, transform);
        };
        prototype.roundRect = function(x, y, width, height, radii = 0) {
            [x, y, width, height] = [x, y, width, height].map(Number);
            if (!canvasPoint([x, y, width, height])) return;
            const corners = canvasRoundRectRadii(radii, width, height);
            if (!corners) return;
            const path = pathFor(this), transform = transformFor(this);
            const paint = (px, py) => transform ? matrixPoint2D(transform, px, py) : [px, py];
            const x0 = Math.min(x, x + width), x1 = Math.max(x, x + width);
            const y0 = Math.min(y, y + height), y1 = Math.max(y, y + height);
            const [ul, ur, lr, ll] = corners;
            moveCanvasPath(path, ...paint(x0 + ul.x, y0));
            lineCanvasPath(path, ...paint(x1 - ur.x, y0));
            ellipseCanvasPath(path, x1 - ur.x, y0 + ur.y, ur.x, ur.y, 0,
                -Math.PI / 2, 0, false, transform);
            lineCanvasPath(path, ...paint(x1, y1 - lr.y));
            ellipseCanvasPath(path, x1 - lr.x, y1 - lr.y, lr.x, lr.y, 0,
                0, Math.PI / 2, false, transform);
            lineCanvasPath(path, ...paint(x0 + ll.x, y1));
            ellipseCanvasPath(path, x0 + ll.x, y1 - ll.y, ll.x, ll.y, 0,
                Math.PI / 2, Math.PI, false, transform);
            lineCanvasPath(path, ...paint(x0, y0 + ul.y));
            ellipseCanvasPath(path, x0 + ul.x, y0 + ul.y, ul.x, ul.y, 0,
                Math.PI, Math.PI * 1.5, false, transform);
            closeCanvasPath(path);
        };
    };
    installCanvasRoundedPathMethods(Path2D.prototype, path => canvasPathData.get(path));
    installCanvasRoundedPathMethods(CanvasRenderingContext2D.prototype,
        context => context.__path, context => context.__transform);
