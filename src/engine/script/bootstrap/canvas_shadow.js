    Object.defineProperties(CanvasRenderingContext2D.prototype, {
        shadowColor: {
            get() { return this.__shadowColor.serialized; },
            set(value) {
                const color = normalizedColor(value);
                if (color) this.__shadowColor = color;
            }
        },
        shadowBlur: {
            get() { return this.__shadowBlur; },
            set(value) {
                value = Number(value);
                if (Number.isFinite(value) && value >= 0) this.__shadowBlur = value;
            }
        },
        shadowOffsetX: {
            get() { return this.__shadowOffsetX; },
            set(value) {
                value = Number(value);
                if (Number.isFinite(value)) this.__shadowOffsetX = value;
            }
        },
        shadowOffsetY: {
            get() { return this.__shadowOffsetY; },
            set(value) {
                value = Number(value);
                if (Number.isFinite(value)) this.__shadowOffsetY = value;
            }
        }
    });
    // A separable box kernel bounds blur work to O(width * height), regardless of radius.
    const canvasBlurAlpha = (pixels, width, height, blur) => {
        const radius = Math.min(Math.ceil(blur), Math.max(width, height));
        const horizontal = new Float32Array(width * height);
        const blurred = new Float32Array(width * height);
        if (!radius) {
            for (let index = 0; index < width * height; index++)
                blurred[index] = pixels[index * 4 + 3];
            return blurred;
        }
        for (let y = 0; y < height; y++) {
            let sum = 0;
            for (let x = 0; x < Math.min(width, radius + 1); x++)
                sum += pixels[(y * width + x) * 4 + 3];
            for (let x = 0; x < width; x++) {
                if (x - radius - 1 >= 0) sum -= pixels[(y * width + x - radius - 1) * 4 + 3];
                if (x + radius < width && x !== 0)
                    sum += pixels[(y * width + x + radius) * 4 + 3];
                horizontal[y * width + x] = sum / (2 * radius + 1);
            }
        }
        for (let x = 0; x < width; x++) {
            let sum = 0;
            for (let y = 0; y < Math.min(height, radius + 1); y++)
                sum += horizontal[y * width + x];
            for (let y = 0; y < height; y++) {
                if (y - radius - 1 >= 0) sum -= horizontal[(y - radius - 1) * width + x];
                if (y + radius < height && y !== 0)
                    sum += horizontal[(y + radius) * width + x];
                blurred[y * width + x] = sum / (2 * radius + 1);
            }
        }
        return blurred;
    };
    const canvasShadowLayer = (context, source, width, height) => {
        const mask = canvasBlurAlpha(source, width, height, context.__shadowBlur);
        const layer = new Uint8ClampedArray(source.length);
        const channels = context.__shadowColor.channels;
        const offsetX = context.__shadowOffsetX, offsetY = context.__shadowOffsetY;
        for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
            const sourceX = x - offsetX, sourceY = y - offsetY;
            const left = Math.floor(sourceX), top = Math.floor(sourceY);
            let alpha = 0;
            for (let dy = 0; dy <= 1; dy++) for (let dx = 0; dx <= 1; dx++) {
                const px = left + dx, py = top + dy;
                if (px < 0 || py < 0 || px >= width || py >= height) continue;
                const weight = (dx ? sourceX - left : 1 - (sourceX - left)) *
                    (dy ? sourceY - top : 1 - (sourceY - top));
                alpha += mask[py * width + px] * weight;
            }
            const destination = (y * width + x) * 4;
            for (let channel = 0; channel < 3; channel++) layer[destination + channel] = channels[channel];
            layer[destination + 3] = alpha * channels[3] / 255;
        }
        return layer;
    };
