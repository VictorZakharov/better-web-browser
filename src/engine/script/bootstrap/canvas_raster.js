    const contextCanvasPath = context => context.__path;
    installCanvasPathMethods(CanvasRenderingContext2D.prototype, contextCanvasPath);
    CanvasRenderingContext2D.prototype.beginPath = function() { this.__path = newCanvasPath(); };
    Object.defineProperty(CanvasRenderingContext2D.prototype, 'strokeStyle', {
        get() { return this.__stroke instanceof CanvasGradient ? this.__stroke : this.__stroke.serialized; },
        set(value) {
            if (value instanceof CanvasGradient) { this.__stroke = value; return; }
            const color = normalizedColor(value); if (color) this.__stroke = color;
        }
    });
    Object.defineProperty(CanvasRenderingContext2D.prototype, 'lineWidth', {
        get() { return this.__lineWidth; },
        set(value) { value = Number(value); if (Number.isFinite(value) && value > 0) this.__lineWidth = value; }
    });
    Object.defineProperty(CanvasRenderingContext2D.prototype, 'lineDashOffset', {
        get() { return this.__dashOffset; },
        set(value) { value = Number(value); if (Number.isFinite(value)) this.__dashOffset = value; }
    });
    CanvasRenderingContext2D.prototype.setLineDash = function(segments) {
        const values = [...segments].map(Number);
        if (values.some(value => !Number.isFinite(value) || value < 0)) return;
        this.__lineDash = values.length % 2 ? values.concat(values) : values;
    };
    CanvasRenderingContext2D.prototype.getLineDash = function() { return [...this.__lineDash]; };
    const canvasPathArgument = (context, candidate) => candidate instanceof Path2D ?
        canvasPathData.get(candidate) : context.__path;
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
                if (length > 0) segments.push({ start, end, length, distance });
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
    const pointOnCanvasStroke = (segments, x, y, width, dash, dashOffset) => {
        for (const segment of segments) {
            const [x0, y0] = segment.start, [x1, y1] = segment.end;
            const deltaX = x1 - x0, deltaY = y1 - y0;
            const projection = ((x - x0) * deltaX + (y - y0) * deltaY) / (segment.length ** 2);
            if (projection < 0 || projection > 1) continue;
            if (Math.hypot(x - (x0 + projection * deltaX), y - (y0 + projection * deltaY)) <= width / 2 &&
                canvasDashVisible(dash, segment.distance + projection * segment.length + dashOffset)) return true;
        }
        return false;
    };
    const paintCanvasPath = (context, path, fill, rule) => {
        const state = stateForCanvas(context.canvas);
        if (!state.pixels) return;
        const inset = fill ? 0 : context.__lineWidth / 2 + 1;
        const [left, top, right, bottom] = canvasPixelBounds(path, state, inset);
        if (!(left < right && top < bottom)) return;
        const style = fill ? context.__fill : context.__stroke;
        if (fill) {
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
                    if (rule === 'evenodd' ? crossings % 2 !== 0 : winding !== 0)
                        compositeCanvasPixel(state.pixels, (y * state.width + x) * 4,
                            canvasPaintAt(style, x + 0.5, y + 0.5),
                            context.__globalAlpha, context.__compositeOperation);
                }
            }
            return;
        }
        // Mark coverage before compositing so joins or overlapping segments do not darken twice.
        const maskWidth = right - left;
        const coverage = new Uint8Array(maskWidth * (bottom - top));
        let coverageWork = 0;
        for (const segment of canvasStrokeSegments(path)) {
            const [[x0, y0], [x1, y1]] = [segment.start, segment.end];
            const radius = context.__lineWidth / 2;
            const minX = Math.max(left, Math.floor(Math.min(x0, x1) - radius));
            const maxX = Math.min(right, Math.ceil(Math.max(x0, x1) + radius));
            const minY = Math.max(top, Math.floor(Math.min(y0, y1) - radius));
            const maxY = Math.min(bottom, Math.ceil(Math.max(y0, y1) + radius));
            coverageWork += Math.max(0, maxX - minX) * Math.max(0, maxY - minY);
            if (coverageWork > 50000000)
                throw new DOMException('Canvas stroke exceeds the raster budget', 'NotSupportedError');
            const deltaX = x1 - x0, deltaY = y1 - y0;
            for (let y = minY; y < maxY; y++) for (let x = minX; x < maxX; x++) {
                const projection = ((x + 0.5 - x0) * deltaX + (y + 0.5 - y0) * deltaY) / (segment.length ** 2);
                if (projection < 0 || projection > 1 ||
                    Math.hypot(x + 0.5 - (x0 + projection * deltaX),
                        y + 0.5 - (y0 + projection * deltaY)) > radius ||
                    !canvasDashVisible(context.__lineDash,
                        segment.distance + projection * segment.length + context.__dashOffset)) continue;
                coverage[(y - top) * maskWidth + x - left] = 1;
            }
        }
        for (let y = top; y < bottom; y++) for (let x = left; x < right; x++) {
            if (coverage[(y - top) * maskWidth + x - left])
                compositeCanvasPixel(state.pixels, (y * state.width + x) * 4,
                    canvasPaintAt(style, x + 0.5, y + 0.5),
                    context.__globalAlpha, context.__compositeOperation);
        }
    };
    CanvasRenderingContext2D.prototype.fill = function(pathOrRule, rule) {
        const path = canvasPathArgument(this, pathOrRule);
        paintCanvasPath(this, path, true, canvasFillRule(pathOrRule instanceof Path2D ? rule : pathOrRule));
    };
    CanvasRenderingContext2D.prototype.stroke = function(path) {
        paintCanvasPath(this, canvasPathArgument(this, path), false, 'nonzero');
    };
    CanvasRenderingContext2D.prototype.isPointInPath = function(pathOrX, xOrY, yOrRule, rule) {
        const external = pathOrX instanceof Path2D;
        const path = external ? canvasPathData.get(pathOrX) : this.__path;
        const x = Number(external ? xOrY : pathOrX), y = Number(external ? yOrRule : xOrY);
        if (!canvasPoint([x, y])) return false;
        return pointInCanvasPath(path, x, y, canvasFillRule(external ? rule : yOrRule));
    };
    CanvasRenderingContext2D.prototype.isPointInStroke = function(pathOrX, xOrY, y) {
        const external = pathOrX instanceof Path2D;
        const path = external ? canvasPathData.get(pathOrX) : this.__path;
        const px = Number(external ? xOrY : pathOrX), py = Number(external ? y : xOrY);
        if (!canvasPoint([px, py])) return false;
        return pointOnCanvasStroke(canvasStrokeSegments(path), px, py, this.__lineWidth,
            this.__lineDash, this.__dashOffset);
    };
