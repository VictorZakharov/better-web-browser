    // Filter Effects Level 1 functions operate on a temporary source image before the
    // Canvas shadow and compositing steps. Unsupported syntax preserves the old filter.
    // https://drafts.fxtf.org/filter-effects-1/#filter-functions
    const canvasFilterNumber = text => {
        const match = /^([+-]?(?:\d+\.?\d*|\.\d+))(%)?$/.exec(text.trim());
        return match ? Number(match[1]) / (match[2] ? 100 : 1) : null;
    };
    const canvasFilterLength = text => {
        if (text.trim() === '0') return 0;
        const match = /^([+-]?(?:\d+\.?\d*|\.\d+))px$/.exec(text.trim());
        return match ? Number(match[1]) : null;
    };
    const canvasFilterAngle = text => {
        if (text.trim() === '0') return 0;
        const match = /^([+-]?(?:\d+\.?\d*|\.\d+))(deg|rad|turn)$/.exec(text.trim());
        if (!match) return null;
        const value = Number(match[1]);
        return match[2] === 'deg' ? value * Math.PI / 180 :
            match[2] === 'turn' ? value * 2 * Math.PI : value;
    };
    const canvasFilterTokens = text => {
        const tokens = [];
        let start = 0, depth = 0;
        for (let index = 0; index <= text.length; index++) {
            if (text[index] === '(') depth++;
            else if (text[index] === ')') depth--;
            if (depth < 0) return null;
            if (index === text.length || (depth === 0 && /\s/.test(text[index]))) {
                const token = text.slice(start, index).trim();
                if (token) tokens.push(token);
                start = index + 1;
            }
        }
        return depth === 0 ? tokens : null;
    };
    const parseCanvasFilters = text => {
        if (text === 'none') return [];
        const operations = [];
        let offset = 0;
        while (offset < text.length) {
            while (/\s/.test(text[offset] || '') && offset < text.length) offset++;
            if (offset >= text.length) break;
            const name = /^[a-z-]+/.exec(text.slice(offset))?.[0];
            if (!name || text[offset + name.length] !== '(') return null;
            offset += name.length + 1;
            const start = offset;
            let depth = 1;
            while (offset < text.length && depth) {
                if (text[offset] === '(') depth++;
                if (text[offset] === ')') depth--;
                offset++;
            }
            if (depth) return null;
            const argument = text.slice(start, offset - 1).trim();
            let value;
            if (name === 'drop-shadow') {
                const tokens = canvasFilterTokens(argument);
                if (!tokens) return null;
                const lengths = tokens.map(token => canvasFilterLength(token)).filter(length => length !== null);
                const colors = tokens.filter(token => canvasFilterLength(token) === null);
                if (lengths.length < 2 || lengths.length > 3 || colors.length > 1 ||
                    (lengths[2] ?? 0) < 0) return null;
                const color = normalizedColor(colors[0] || '#000000');
                if (!color) return null;
                value = {x: lengths[0], y: lengths[1], blur: lengths[2] ?? 0, color};
            } else if (name === 'blur') value = canvasFilterLength(argument || '0px');
            else if (name === 'hue-rotate') value = canvasFilterAngle(argument || '0deg');
            else if (['brightness', 'contrast', 'grayscale', 'invert', 'opacity',
                'saturate', 'sepia'].includes(name)) {
                value = canvasFilterNumber(argument ||
                    (['grayscale', 'invert', 'sepia'].includes(name) ? '1' : '1'));
            } else return null;
            if (value === null || (name !== 'drop-shadow' && (!Number.isFinite(value) ||
                (name !== 'hue-rotate' && value < 0)))) return null;
            operations.push({name, value});
            if (offset < text.length && !/\s/.test(text[offset])) return null;
        }
        return operations.length ? operations : null;
    };
    Object.defineProperty(CanvasRenderingContext2D.prototype, 'filter', {
        get() { return this.__filter; },
        set(value) {
            const text = String(value).trim();
            const operations = parseCanvasFilters(text);
            if (operations) { this.__filter = text; this.__filterOperations = operations; }
        }
    });
    const canvasFilterBlur = (pixels, width, height, blur) => {
        const radius = Math.min(Math.ceil(blur * 1.5), Math.max(width, height));
        if (!radius) return pixels;
        const horizontal = new Float32Array(pixels.length);
        const result = new Uint8ClampedArray(pixels.length);
        for (let y = 0; y < height; y++) {
            const sums = [0, 0, 0, 0];
            const add = (x, sign) => {
                if (x < 0 || x >= width) return;
                const index = (y * width + x) * 4, alpha = pixels[index + 3] / 255;
                for (let channel = 0; channel < 3; channel++)
                    sums[channel] += sign * pixels[index + channel] * alpha;
                sums[3] += sign * pixels[index + 3];
            };
            for (let x = 0; x <= radius; x++) add(x, 1);
            for (let x = 0; x < width; x++) {
                if (x) { add(x - radius - 1, -1); add(x + radius, 1); }
                const index = (y * width + x) * 4;
                for (let channel = 0; channel < 4; channel++)
                    horizontal[index + channel] = sums[channel] / (2 * radius + 1);
            }
        }
        for (let x = 0; x < width; x++) {
            const sums = [0, 0, 0, 0];
            const add = (y, sign) => {
                if (y < 0 || y >= height) return;
                const index = (y * width + x) * 4;
                for (let channel = 0; channel < 4; channel++)
                    sums[channel] += sign * horizontal[index + channel];
            };
            for (let y = 0; y <= radius; y++) add(y, 1);
            for (let y = 0; y < height; y++) {
                if (y) { add(y - radius - 1, -1); add(y + radius, 1); }
                const index = (y * width + x) * 4;
                const alpha = sums[3] / (2 * radius + 1);
                result[index + 3] = alpha;
                for (let channel = 0; channel < 3; channel++)
                    result[index + channel] = alpha === 0 ? 0 :
                        sums[channel] / (2 * radius + 1) * 255 / alpha;
            }
        }
        return result;
    };
    const canvasFilterPixel = (pixels, offset, name, amount) => {
        let r = pixels[offset], g = pixels[offset + 1], b = pixels[offset + 2];
        if (name === 'opacity') { pixels[offset + 3] *= Math.min(1, amount); return; }
        if (name === 'brightness') [r, g, b] = [r, g, b].map(value => value * amount);
        else if (name === 'contrast') [r, g, b] = [r, g, b].map(value =>
            ((value / 255 - 0.5) * amount + 0.5) * 255);
        else if (name === 'invert') [r, g, b] = [r, g, b].map(value =>
            value * (1 - Math.min(1, amount)) + (255 - value) * Math.min(1, amount));
        else if (name === 'grayscale' || name === 'saturate') {
            const luminance = .2126 * r + .7152 * g + .0722 * b;
            const scale = name === 'grayscale' ? 1 - Math.min(1, amount) : amount;
            [r, g, b] = [r, g, b].map(value => luminance + (value - luminance) * scale);
        } else if (name === 'sepia') {
            const scale = Math.min(1, amount);
            [r, g, b] = [r * (1 - scale) + scale * (.393*r + .769*g + .189*b),
                g * (1 - scale) + scale * (.349*r + .686*g + .168*b),
                b * (1 - scale) + scale * (.272*r + .534*g + .131*b)];
        } else if (name === 'hue-rotate') {
            const cosine = Math.cos(amount), sine = Math.sin(amount);
            [r, g, b] = [
                (.213 + .787*cosine - .213*sine)*r + (.715 - .715*cosine - .715*sine)*g +
                    (.072 - .072*cosine + .928*sine)*b,
                (.213 - .213*cosine + .143*sine)*r + (.715 + .285*cosine + .140*sine)*g +
                    (.072 - .072*cosine - .283*sine)*b,
                (.213 - .213*cosine - .787*sine)*r + (.715 - .715*cosine + .715*sine)*g +
                    (.072 + .928*cosine + .072*sine)*b
            ];
        }
        pixels[offset] = r; pixels[offset + 1] = g; pixels[offset + 2] = b;
    };
    const applyCanvasFilters = (pixels, width, height, operations) => {
        let result = pixels;
        for (const {name, value} of operations) {
            if (name === 'blur') { result = canvasFilterBlur(result, width, height, value); continue; }
            if (name === 'drop-shadow') {
                const shadow = canvasShadowLayer({__shadowBlur: value.blur,
                    __shadowOffsetX: value.x, __shadowOffsetY: value.y,
                    __shadowColor: value.color}, result, width, height);
                for (let offset = 0; offset < result.length; offset += 4)
                    compositeCanvasPixel(shadow, offset, result.subarray(offset, offset + 4),
                        1, 'source-over');
                result = shadow;
                continue;
            }
            for (let offset = 0; offset < result.length; offset += 4)
                canvasFilterPixel(result, offset, name, value);
        }
        return result;
    };
