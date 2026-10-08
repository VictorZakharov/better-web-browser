    defineCanvasContextProperties(CanvasRenderingContext2D.prototype, {
        shadowColor: {
            get() { return canvasDrawingState(this).shadowColor.serialized; },
            set(value) {
                const color = normalizedColor(value);
                if (color) canvasDrawingState(this).shadowColor = color;
            }
        },
        shadowBlur: {
            get() { return canvasDrawingState(this).shadowBlur; },
            set(value) {
                value = +value;
                if (Number.isFinite(value) && value >= 0) canvasDrawingState(this).shadowBlur = value;
            }
        },
        shadowOffsetX: {
            get() { return canvasDrawingState(this).shadowOffsetX; },
            set(value) {
                value = +value;
                if (Number.isFinite(value)) canvasDrawingState(this).shadowOffsetX = value;
            }
        },
        shadowOffsetY: {
            get() { return canvasDrawingState(this).shadowOffsetY; },
            set(value) {
                value = +value;
                if (Number.isFinite(value)) canvasDrawingState(this).shadowOffsetY = value;
            }
        }
    });
    // A separable box kernel bounds blur work to O(width * height), regardless of radius.
    const canvasBlurAlpha = (pixels, width, height, blur) => {
        const radius = canvasPrivateMath.min(canvasPrivateMath.ceil(blur), canvasPrivateMath.max(width, height));
        const horizontal = new canvasPrivateFloatArray(width * height);
        const blurred = new canvasPrivateFloatArray(width * height);
        if (!radius) {
            for (let index = 0; index < width * height; index++)
                blurred[index] = pixels[index * 4 + 3];
            return blurred;
        }
        for (let y = 0; y < height; y++) {
            let sum = 0;
            for (let x = 0; x < canvasPrivateMath.min(width, radius + 1); x++)
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
            for (let y = 0; y < canvasPrivateMath.min(height, radius + 1); y++)
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
    const canvasShadowHost = __hostCall;
    const canvasShadowLayer = (context, source, width, height) =>
        canvasShadowLayerFromState(canvasDrawingState(context), source, width, height);
    const canvasShadowLayerFromState = (settings, source, width, height) => {
        const nativeLayer = canvasShadowHost('canvasShadowLayer', source, width, height,
            settings.shadowBlur, settings.shadowOffsetX, settings.shadowOffsetY,
            canvasPrivateColorBytes(settings.shadowColor.channels));
        if (nativeLayer) return new canvasPrivatePixelArray(canvasPrivateBuffer(nativeLayer),
            canvasPrivateOffset(nativeLayer), canvasPrivateByteLength(nativeLayer));
        const mask = canvasBlurAlpha(source, width, height, settings.shadowBlur);
        const layer = new canvasPrivatePixelArray(canvasPrivateCount(source));
        const channels = settings.shadowColor.channels;
        const offsetX = settings.shadowOffsetX, offsetY = settings.shadowOffsetY;
        for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
            const sourceX = x - offsetX, sourceY = y - offsetY;
            const left = canvasPrivateMath.floor(sourceX), top = canvasPrivateMath.floor(sourceY);
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
