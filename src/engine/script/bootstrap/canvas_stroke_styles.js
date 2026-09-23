    Object.defineProperties(CanvasRenderingContext2D.prototype, {
        lineCap: {
            get() { return this.__lineCap; },
            set(value) { if (['butt', 'round', 'square'].includes(value)) this.__lineCap = value; }
        },
        lineJoin: {
            get() { return this.__lineJoin; },
            set(value) { if (['round', 'bevel', 'miter'].includes(value)) this.__lineJoin = value; }
        },
        miterLimit: {
            get() { return this.__miterLimit; },
            set(value) {
                value = Number(value);
                if (Number.isFinite(value) && value > 0) this.__miterLimit = value;
            }
        }
    });
    const canvasStrokeSegmentContains = (segment, x, y, radius, cap) => {
        const [x0, y0] = segment.start, [x1, y1] = segment.end;
        const dx = x1 - x0, dy = y1 - y0;
        const projection = ((x - x0) * dx + (y - y0) * dy) / (segment.length ** 2);
        if (projection < 0) {
            if (!segment.startCap || cap === 'butt') return null;
            if (cap === 'round') return Math.hypot(x - x0, y - y0) <= radius ? 0 : null;
            if (projection < -radius / segment.length) return null;
        } else if (projection > 1) {
            if (!segment.endCap || cap === 'butt') return null;
            if (cap === 'round') return Math.hypot(x - x1, y - y1) <= radius ? 1 : null;
            if (projection > 1 + radius / segment.length) return null;
        }
        const distance = Math.abs((x - x0) * dy - (y - y0) * dx) / segment.length;
        return distance <= radius ? Math.max(0, Math.min(1, projection)) : null;
    };
    const canvasTriangleContains = (point, a, b, c) => {
        const side = (u, v) => (point[0] - u[0]) * (v[1] - u[1]) -
            (point[1] - u[1]) * (v[0] - u[0]);
        const first = side(a, b), second = side(b, c), third = side(c, a);
        return (first >= 0 && second >= 0 && third >= 0) ||
            (first <= 0 && second <= 0 && third <= 0);
    };
    const canvasStrokeJoins = path => {
        const joins = [];
        for (const part of path.subpaths) {
            const points = part.points, count = points.length;
            if (count < 3) continue;
            for (let index = part.closed ? 0 : 1;
                index < (part.closed ? count : count - 1); index++) {
                joins.push([points[(index + count - 1) % count], points[index],
                    points[(index + 1) % count]]);
            }
        }
        return joins;
    };
    const canvasJoinCovers = (previous, point, next, x, y, radius, join, miterLimit) => {
        const firstLength = Math.hypot(point[0] - previous[0], point[1] - previous[1]);
        const secondLength = Math.hypot(next[0] - point[0], next[1] - point[1]);
        if (!firstLength || !secondLength) return false;
        const u = [(point[0] - previous[0]) / firstLength,
            (point[1] - previous[1]) / firstLength];
        const v = [(next[0] - point[0]) / secondLength,
            (next[1] - point[1]) / secondLength];
        const cross = u[0] * v[1] - u[1] * v[0];
        if (Math.abs(cross) < 1e-12) return false;
        if (join === 'round') return Math.hypot(x - point[0], y - point[1]) <= radius;
        const direction = cross > 0 ? 1 : -1;
        const a = [point[0] + direction * u[1] * radius,
            point[1] - direction * u[0] * radius];
        const b = [point[0] + direction * v[1] * radius,
            point[1] - direction * v[0] * radius];
        const sample = [x, y];
        if (canvasTriangleContains(sample, point, a, b)) return true;
        if (join !== 'miter') return false;
        const difference = [b[0] - a[0], b[1] - a[1]];
        const distance = (difference[0] * v[1] - difference[1] * v[0]) / cross;
        const tip = [a[0] + distance * u[0], a[1] + distance * u[1]];
        return Math.hypot(tip[0] - point[0], tip[1] - point[1]) <=
            miterLimit * radius && canvasTriangleContains(sample, a, tip, b);
    };
