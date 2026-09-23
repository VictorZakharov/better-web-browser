    // Canvas 2D gradients are live paint objects: stops can change after assignment to fillStyle.
    const canvasGradientToken = Symbol('CanvasGradient');
    class CanvasGradient {
        constructor(token, kind, geometry) {
            if (token !== canvasGradientToken) throw new TypeError('Illegal constructor');
            this.__kind = kind;
            this.__geometry = geometry;
            this.__stops = [];
        }
        addColorStop(offset, color) {
            offset = Number(offset);
            if (!Number.isFinite(offset) || offset < 0 || offset > 1)
                throw new DOMException('Color stop offset must be in [0, 1]', 'IndexSizeError');
            const resolved = normalizedColor(color);
            if (!resolved) throw new DOMException('Invalid Canvas gradient color', 'SyntaxError');
            this.__stops.push({ offset, channels: resolved.channels });
            this.__stops.sort((left, right) => left.offset - right.offset);
        }
    }
    const canvasGradient = (kind, geometry) => {
        geometry = geometry.map(Number);
        if (!geometry.every(Number.isFinite)) throw new DOMException('Non-finite gradient geometry', 'NotSupportedError');
        if (kind === 'radial' && (geometry[2] < 0 || geometry[5] < 0))
            throw new DOMException('Gradient radii must be non-negative', 'IndexSizeError');
        return new CanvasGradient(canvasGradientToken, kind, geometry);
    };
    const gradientPosition = (gradient, x, y) => {
        const coordinates = gradient.__geometry;
        if (gradient.__kind === 'linear') {
            const [x0, y0, x1, y1] = coordinates;
            const dx = x1 - x0, dy = y1 - y0;
            const lengthSquared = dx * dx + dy * dy;
            return lengthSquared === 0 ? null : ((x - x0) * dx + (y - y0) * dy) / lengthSquared;
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
    const canvasPaintAt = (style, x, y) => {
        if (!(style instanceof CanvasGradient)) return style.channels;
        const stops = style.__stops;
        const position = gradientPosition(style, x, y);
        if (!stops.length || position === null) return [0, 0, 0, 0];
        if (position < stops[0].offset) return stops[0].channels;
        if (position >= stops[stops.length - 1].offset) return stops[stops.length - 1].channels;
        let upper = 1;
        while (upper < stops.length && stops[upper].offset <= position) upper++;
        const lower = stops[upper - 1], next = stops[upper];
        const portion = (position - lower.offset) / (next.offset - lower.offset);
        const alpha = lower.channels[3] + (next.channels[3] - lower.channels[3]) * portion;
        if (alpha === 0) return [0, 0, 0, 0];
        const channels = [0, 0, 0, alpha];
        for (let index = 0; index < 3; index++) {
            const first = lower.channels[index] * lower.channels[3];
            const second = next.channels[index] * next.channels[3];
            channels[index] = (first * (1 - portion) + second * portion) / alpha;
        }
        return channels;
    };
