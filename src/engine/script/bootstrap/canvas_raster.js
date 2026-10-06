    const canvasRasterHost = __hostCall;
    const canvasRasterStringify = canvasPrivateWireStringify;
    const contextCanvasPath = context => canvasDrawingState(context).path;
    installCanvasPathMethods(CanvasRenderingContext2D.prototype, contextCanvasPath,
        context => canvasDrawingState(context).transform);
    CanvasRenderingContext2D.prototype.beginPath = function() { canvasDrawingState(this).path = newCanvasPath(); };
    Object.defineProperty(CanvasRenderingContext2D.prototype, 'strokeStyle', {
        configurable:true, enumerable:true,
        get() { return canvasIsGradient(canvasDrawingState(this).stroke) ||
            canvasIsPattern(canvasDrawingState(this).stroke) ? canvasDrawingState(this).stroke : canvasDrawingState(this).stroke.serialized; },
        set(value) {
            if (canvasIsGradient(value) || canvasIsPattern(value)) {
                canvasDrawingState(this).stroke = value; return;
            }
            const color = normalizedColor(value); if (color) canvasDrawingState(this).stroke = color;
        }
    });
    const canvasPathArgument = (context, candidate) => canvasPathData.has(candidate) ?
        transformCanvasPath(canvasPathData.get(candidate), canvasDrawingState(context).transform) : canvasDrawingState(context).path;
    const canvasFillRule = value => value === 'evenodd' ? 'evenodd' : 'nonzero';
    const pathEdges = path => {
        const edges = [];
        for (const part of path.subpaths) {
            const points = part.points;
            if (points.length < 2) continue;
            for (let index = 1; index < points.length; index++)
                edges.push([points[index - 1], points[index]]);
            edges.push([points[points.length - 1], points[0]]);
        }
        return edges;
    };
    const pointInCanvasPath = (path, x, y, rule) => {
        let crossings = 0, winding = 0;
        for (const [[x0, y0], [x1, y1]] of pathEdges(path)) {
            if ((y0 <= y && y1 > y) || (y1 <= y && y0 > y)) {
                const intersection = x0 + (y - y0) * (x1 - x0) / (y1 - y0);
                if (intersection > x) {
                    crossings++;
                    winding += y1 > y0 ? 1 : -1;
                }
            }
        }
        return rule === 'evenodd' ? crossings % 2 !== 0 : winding !== 0;
    };
    const pathBounds = path => {
        let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
        for (const part of path.subpaths) for (const [x, y] of part.points) {
            minX = Math.min(minX, x); minY = Math.min(minY, y);
            maxX = Math.max(maxX, x); maxY = Math.max(maxY, y);
        }
        return [minX, minY, maxX, maxY];
    };
    const canvasPixelBounds = (path, state, inset) => {
        const [minX, minY, maxX, maxY] = pathBounds(path);
        return [Math.max(0, Math.floor(minX - inset)), Math.max(0, Math.floor(minY - inset)),
            Math.min(state.width, Math.ceil(maxX + inset)), Math.min(state.height, Math.ceil(maxY + inset))];
    };
    const canvasStrokeSegments = path => {
        const segments = [];
        let distance = 0;
        for (const part of path.subpaths) {
            const points = part.points;
            const count = points.length - 1 + (part.closed && points.length > 1 ? 1 : 0);
            for (let index = 0; index < count; index++) {
                const start = points[index], end = points[(index + 1) % points.length];
                const length = Math.hypot(end[0] - start[0], end[1] - start[1]);
                if (length > 0) segments.push({ start, end, length, distance,
                    startCap: !part.closed && index === 0,
                    endCap: !part.closed && index === count - 1 });
                distance += length;
            }
        }
        return segments;
    };
    const canvasDashVisible = (segments, distance) => {
        if (!segments.length) return true;
        const period = segments.reduce((sum, segment) => sum + segment, 0);
        if (period === 0) return true;
        distance = ((distance % period) + period) % period;
        for (let index = 0; index < segments.length; index++) {
            if (distance < segments[index]) return index % 2 === 0;
            distance -= segments[index];
        }
        return true;
    };
    const pointOnCanvasStroke = (path, segments, x, y, width, dash, dashOffset,
        cap, join, miterLimit) => {
        for (const segment of segments) {
            const projection = canvasStrokeSegmentContains(segment, x, y, width / 2, cap);
            if (projection !== null &&
                canvasDashVisible(dash, segment.distance + projection * segment.length + dashOffset)) return true;
        }
        return canvasStrokeJoins(path).some(([previous, point, next]) =>
            canvasJoinCovers(previous, point, next, x, y, width / 2, join, miterLimit));
    };
    const canvasNativeStrokeRequest = (context, path, width, height, left, top) => canvasRasterStringify({
        width, height, left, top, line_width: canvasDrawingState(context).lineWidth, miter_limit: canvasDrawingState(context).miterLimit,
        cap: canvasDrawingState(context).lineCap, join: canvasDrawingState(context).lineJoin, transform: canvasDrawingState(context).transform,
        dash: canvasDrawingState(context).lineDash, dash_offset: canvasDrawingState(context).dashOffset,
        antialias: true,
        parts: path.subpaths.map(part => ({ points: part.points, closed: !!part.closed }))
    });
    const paintCanvasPath = (context, path, fill, rule) => {
        const state = stateForCanvas(context.canvas);
        if (!state.pixels) return;
        const paintInverse = matrixInverse2D(canvasDrawingState(context).transform);
        if (!fill && !paintInverse) return;
        const [a, b, c, d] = canvasDrawingState(context).transform;
        const penScale = Math.max(Math.hypot(a, c), Math.hypot(b, d));
        const inset = fill ? 0 : canvasDrawingState(context).lineWidth / 2 *
            penScale * (canvasDrawingState(context).lineJoin === 'miter' ? canvasDrawingState(context).miterLimit : 1) + 1;
        const [left, top, right, bottom] = canvasPixelBounds(path, state, inset);
        if (!(left < right && top < bottom)) return;
        const style = fill ? canvasDrawingState(context).fill : canvasDrawingState(context).stroke;
        if (!paintInverse && (canvasIsGradient(style) || canvasIsPattern(style))) return;
        if (fill) {
            const request = canvasRasterStringify({
                width: right - left, height: bottom - top, left, top, rule,
                parts: path.subpaths.map(part => ({points: part.points, closed: !!part.closed}))
            });
            if (canvasPaintSolidPath(context,state,'fill',request,style,left,top,right,bottom)) return;
            const nativeFill = canvasRasterHost('canvasFillMask', request);
            if (nativeFill) {
                if (canvasPaintSolidMask(context, state, nativeFill, style, left, top, right, bottom)) return;
                for (let y = top; y < bottom; y++) for (let x = left; x < right; x++) {
                    const coverage = nativeFill[(y - top) * (right - left) + x - left] / 255;
                    if (coverage && canvasClipAllows(context, x, y, state.width))
                        compositeCanvasPixel(state.pixels, (y * state.width + x) * 4,
                            canvasPaintAt(style, x + .5, y + .5, paintInverse),
                            canvasDrawingState(context).globalAlpha * coverage, canvasDrawingState(context).compositeOperation);
                }
                return;
            }
            const edges = pathEdges(path);
            for (let y = top; y < bottom; y++) {
                const intersections = [];
                const sampleY = y + 0.5;
                for (const [[x0, y0], [x1, y1]] of edges) {
                    if ((y0 <= sampleY && y1 > sampleY) || (y1 <= sampleY && y0 > sampleY))
                        intersections.push([x0 + (sampleY - y0) * (x1 - x0) / (y1 - y0), y1 > y0 ? 1 : -1]);
                }
                intersections.sort((a, b) => a[0] - b[0]);
                let edge = 0, crossings = 0, winding = 0;
                for (let x = left; x < right; x++) {
                    while (edge < intersections.length && intersections[edge][0] <= x + 0.5) {
                        crossings++; winding += intersections[edge++][1];
                    }
                    if (!canvasClipAllows(context, x, y, state.width)) continue;
                    if (rule === 'evenodd' ? crossings % 2 !== 0 : winding !== 0)
                        compositeCanvasPixel(state.pixels, (y * state.width + x) * 4,
                            canvasPaintAt(style, x + 0.5, y + 0.5, paintInverse),
                            canvasDrawingState(context).globalAlpha, canvasDrawingState(context).compositeOperation);
                }
            }
            return;
        }
        // Mark coverage before compositing so joins or overlapping segments do not darken twice.
        const maskWidth = right - left;
        // The default path already contains construction-time transformed points.
        // Undo only the painting CTM, stroke with its pen, then transform back.
        const strokePath = transformCanvasPath(path, paintInverse);
        const request = canvasNativeStrokeRequest(context, strokePath, maskWidth, bottom-top, left, top);
        if (canvasPaintSolidPath(context,state,'stroke',request,style,left,top,right,bottom)) return;
        const nativeCoverage = canvasRasterHost('canvasStrokeMask', request);
        const coverage = nativeCoverage || new Uint8Array(maskWidth * (bottom - top));
        if (nativeCoverage && canvasPaintSolidMask(context, state, nativeCoverage, style,
            left, top, right, bottom)) return;
        if (!nativeCoverage && !canvasIsIdentity(canvasDrawingState(context).transform)) {
            const segments = canvasStrokeSegments(strokePath);
            const work = maskWidth * (bottom - top) * Math.max(1, strokePath.pointCount);
            if (work > 50000000)
                throw new DOMException('Canvas stroke exceeds the raster budget', 'NotSupportedError');
            for (let y = top; y < bottom; y++) for (let x = left; x < right; x++) {
                const [px, py] = matrixPoint2D(paintInverse, x + .5, y + .5);
                if (pointOnCanvasStroke(strokePath, segments, px, py, canvasDrawingState(context).lineWidth,
                    canvasDrawingState(context).lineDash, canvasDrawingState(context).dashOffset, canvasDrawingState(context).lineCap,
                    canvasDrawingState(context).lineJoin, canvasDrawingState(context).miterLimit))
                    coverage[(y - top) * maskWidth + x - left] = 1;
            }
        } else if (!nativeCoverage) {
            let coverageWork = 0;
            for (const segment of canvasStrokeSegments(path)) {
                const [[x0, y0], [x1, y1]] = [segment.start, segment.end];
                const radius = canvasDrawingState(context).lineWidth / 2;
                const minX = Math.max(left, Math.floor(Math.min(x0, x1) - radius));
                const maxX = Math.min(right, Math.ceil(Math.max(x0, x1) + radius));
                const minY = Math.max(top, Math.floor(Math.min(y0, y1) - radius));
                const maxY = Math.min(bottom, Math.ceil(Math.max(y0, y1) + radius));
                coverageWork += Math.max(0, maxX - minX) * Math.max(0, maxY - minY);
                if (coverageWork > 50000000)
                    throw new DOMException('Canvas stroke exceeds the raster budget', 'NotSupportedError');
                for (let y = minY; y < maxY; y++) for (let x = minX; x < maxX; x++) {
                    const projection = canvasStrokeSegmentContains(segment,
                        x + 0.5, y + 0.5, radius, canvasDrawingState(context).lineCap);
                    if (projection === null ||
                        !canvasDashVisible(canvasDrawingState(context).lineDash,
                            segment.distance + projection * segment.length + canvasDrawingState(context).dashOffset)) continue;
                    coverage[(y - top) * maskWidth + x - left] = 1;
                }
            }
            for (const [previous, point, next] of canvasStrokeJoins(path)) {
                const radius = canvasDrawingState(context).lineWidth / 2;
                const extent = radius * (canvasDrawingState(context).lineJoin === 'miter' ? canvasDrawingState(context).miterLimit : 1);
                const minX = Math.max(left, Math.floor(point[0] - extent));
                const maxX = Math.min(right, Math.ceil(point[0] + extent));
                const minY = Math.max(top, Math.floor(point[1] - extent));
                const maxY = Math.min(bottom, Math.ceil(point[1] + extent));
                coverageWork += Math.max(0, maxX - minX) * Math.max(0, maxY - minY);
                if (coverageWork > 50000000)
                    throw new DOMException('Canvas stroke exceeds the raster budget', 'NotSupportedError');
                for (let y = minY; y < maxY; y++) for (let x = minX; x < maxX; x++)
                    if (canvasJoinCovers(previous, point, next, x + 0.5, y + 0.5,
                        radius, canvasDrawingState(context).lineJoin, canvasDrawingState(context).miterLimit))
                        coverage[(y - top) * maskWidth + x - left] = 1;
            }
        }
        for (let y = top; y < bottom; y++) for (let x = left; x < right; x++) {
            if (coverage[(y - top) * maskWidth + x - left] &&
                canvasClipAllows(context, x, y, state.width))
                compositeCanvasPixel(state.pixels, (y * state.width + x) * 4,
                    canvasPaintAt(style, x + 0.5, y + 0.5, paintInverse),
                    canvasDrawingState(context).globalAlpha * (nativeCoverage ?
                        coverage[(y - top) * maskWidth + x - left] / 255 : 1),
                    canvasDrawingState(context).compositeOperation);
        }
    };
    CanvasRenderingContext2D.prototype.fill = function(pathOrRule, rule) {
        const path = canvasPathArgument(this, pathOrRule);
        paintCanvasPath(this, path, true, canvasFillRule(canvasPathData.has(pathOrRule) ? rule : pathOrRule));
    };
    CanvasRenderingContext2D.prototype.stroke = function(path) {
        paintCanvasPath(this, canvasPathArgument(this, path), false, 'nonzero');
    };
    CanvasRenderingContext2D.prototype.isPointInPath = function(pathOrX, xOrY, yOrRule, rule) {
        const external = canvasPathData.has(pathOrX);
        const path = canvasPathArgument(this, pathOrX);
        const x = Number(external ? xOrY : pathOrX), y = Number(external ? yOrRule : xOrY);
        if (!canvasPoint([x, y])) return false;
        return pointInCanvasPath(path, x, y, canvasFillRule(external ? rule : yOrRule));
    };
    CanvasRenderingContext2D.prototype.isPointInStroke = function(pathOrX, xOrY, y) {
        const external = canvasPathData.has(pathOrX);
        const path = canvasPathArgument(this, pathOrX);
        const px = Number(external ? xOrY : pathOrX), py = Number(external ? y : xOrY);
        if (!canvasPoint([px, py])) return false;
        const inverse = matrixInverse2D(canvasDrawingState(this).transform);
        if (!inverse) return false;
        const strokePath = transformCanvasPath(path, inverse);
        const nativeHit = canvasDrawingState(this).lineDash.length ? canvasRasterHost('canvasStrokeContains',
            canvasNativeStrokeRequest(this, strokePath, 1, 1, 0, 0), px, py) : null;
        if (nativeHit !== null) return nativeHit;
        const [localX, localY] = matrixPoint2D(inverse, px, py);
        return pointOnCanvasStroke(strokePath, canvasStrokeSegments(strokePath), localX, localY, canvasDrawingState(this).lineWidth,
            canvasDrawingState(this).lineDash, canvasDrawingState(this).dashOffset, canvasDrawingState(this).lineCap, canvasDrawingState(this).lineJoin, canvasDrawingState(this).miterLimit);
    };
