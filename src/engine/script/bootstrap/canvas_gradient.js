    // Canvas 2D gradients are live paint objects: stops can change after assignment to fillStyle.
    const canvasGradientToken = Symbol('CanvasGradient');
    const canvasGradientStates = new WeakMap();
    const canvasGradientGet = Function.call.bind(WeakMap.prototype.get);
    const canvasGradientSet = Function.call.bind(WeakMap.prototype.set);
    const canvasGradientHas = Function.call.bind(WeakMap.prototype.has);
    const canvasGradientPush = Function.call.bind(Array.prototype.push);
    const canvasGradientSort = Function.call.bind(Array.prototype.sort);
    const canvasGradientArray = Array;
    const canvasGradientSetPrototype = Object.setPrototypeOf;
    const canvasGradientOwnArray = length =>
        canvasGradientSetPrototype(new canvasGradientArray(length), null);
    const canvasIsGradient = value => canvasGradientHas(canvasGradientStates, value);
    class CanvasGradient {
        constructor(token, kind, geometry) {
            if (token !== canvasGradientToken) throw new TypeError('Illegal constructor');
            canvasGradientSet(canvasGradientStates, this, {
                kind, geometry, stops: canvasGradientOwnArray(0)
            });
        }
        addColorStop(offset, color) {
            const state = canvasGradientGet(canvasGradientStates, this);
            if (!state) throw new TypeError('addColorStop requires a CanvasGradient');
            if (arguments.length < 2) throw new TypeError('addColorStop requires two arguments');
            offset = +offset;
            if (!Number.isFinite(offset)) throw new TypeError('Color stop offset must be finite');
            color = `${color}`;
            if (offset < 0 || offset > 1)
                throw new DOMException('Color stop offset must be in [0, 1]', 'IndexSizeError');
            const resolved = normalizedColor(color);
            if (!resolved) throw new DOMException('Invalid Canvas gradient color', 'SyntaxError');
            canvasGradientPush(state.stops, { offset, channels: resolved.channels });
            canvasGradientSort(state.stops, (left, right) => left.offset - right.offset);
        }
    }
    Object.defineProperty(CanvasGradient.prototype, Symbol.toStringTag, {
        value: 'CanvasGradient', configurable: true
    });
    Object.defineProperty(CanvasGradient.prototype, 'addColorStop', {enumerable: true});
    const canvasGradient = (kind, geometry) => {
        for (let index = 0; index < geometry.length; index++) {
            const number = +geometry[index];
            if (!Number.isFinite(number)) throw new TypeError('Gradient geometry must be finite');
            geometry[index] = number;
        }
        if (kind === 'radial' && (geometry[2] < 0 || geometry[5] < 0))
            throw new DOMException('Gradient radii must be non-negative', 'IndexSizeError');
        return new CanvasGradient(canvasGradientToken, kind, geometry);
    };
    const gradientPosition = (gradient, x, y) => {
        const coordinates = gradient.geometry;
        if (gradient.kind === 'linear') {
            const [x0, y0, x1, y1] = coordinates;
            const dx = x1 - x0, dy = y1 - y0;
            const lengthSquared = dx * dx + dy * dy;
            return lengthSquared === 0 ? null : ((x - x0) * dx + (y - y0) * dy) / lengthSquared;
        }
        if (gradient.kind === 'conic') {
            const [startAngle, centerX, centerY] = coordinates;
            const turn = (Math.atan2(y - centerY, x - centerX) - startAngle % (2 * Math.PI)) / (2 * Math.PI);
            return ((turn % 1) + 1) % 1;
        }
        const [x0, y0, r0, x1, y1, r1] = coordinates;
        const dx = x1 - x0, dy = y1 - y0, dr = r1 - r0;
        const px = x - x0, py = y - y0;
        const a = dx * dx + dy * dy - dr * dr;
        const b = -2 * (px * dx + py * dy + r0 * dr);
        const c = px * px + py * py - r0 * r0;
        if (Math.abs(a) < 1e-12) return b === 0 ? null : -c / b;
        const discriminant = b * b - 4 * a * c;
        if (discriminant < 0) return null;
        const root = Math.sqrt(discriminant);
        const first = (-b - root) / (2 * a), second = (-b + root) / (2 * a);
        const valid = [first, second].filter(t => r0 + t * dr >= 0);
        return valid.length ? Math.max(...valid) : null;
    };
    const canvasPaintAt = (style, x, y, inverse = null) => {
        if (inverse && (canvasIsGradient(style) || canvasIsPattern(style)))
            [x, y] = matrixPoint2D(inverse, x, y);
        if (canvasIsPattern(style)) return sampleCanvasPattern(style, x, y);
        const gradient = canvasGradientGet(canvasGradientStates, style);
        if (!gradient) return style.channels;
        const stops = gradient.stops;
        const position = gradientPosition(gradient, x, y);
        if (!stops.length || position === null) return [0, 0, 0, 0];
        if (position < stops[0].offset) return stops[0].channels;
        if (position >= stops[stops.length - 1].offset) return stops[stops.length - 1].channels;
        let upper = 1;
        while (upper < stops.length && stops[upper].offset <= position) upper++;
        const lower = stops[upper - 1], next = stops[upper];
        const portion = (position - lower.offset) / (next.offset - lower.offset);
        // HTML Canvas interpolates color and alpha independently, without
        // premultiplication. Premultiply only during subsequent compositing.
        const channels = [0, 0, 0, 0];
        for (let index = 0; index < 4; index++)
            channels[index] = lower.channels[index] +
                (next.channels[index] - lower.channels[index]) * portion;
        return channels;
    };
