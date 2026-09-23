    // Shared bounded image sampling for ImageBitmap resize and Canvas drawImage.
    const sampleCanvasBitmap = (source, x, y, smooth) => {
        if (!smooth) {
            const column = Math.max(0, Math.min(source.width - 1, Math.round(x)));
            const row = Math.max(0, Math.min(source.height - 1, Math.round(y)));
            return source.pixels.subarray((row * source.width + column) * 4,
                (row * source.width + column) * 4 + 4);
        }
        const left = Math.floor(x), top = Math.floor(y);
        const fractionX = x - left, fractionY = y - top;
        const samples = [
            [left, top, (1 - fractionX) * (1 - fractionY)],
            [left + 1, top, fractionX * (1 - fractionY)],
            [left, top + 1, (1 - fractionX) * fractionY],
            [left + 1, top + 1, fractionX * fractionY]
        ];
        let alpha = 0;
        const premultiplied = [0, 0, 0];
        for (const [column, row, weight] of samples) {
            const clampedX = Math.max(0, Math.min(source.width - 1, column));
            const clampedY = Math.max(0, Math.min(source.height - 1, row));
            const offset = (clampedY * source.width + clampedX) * 4;
            const sampleAlpha = source.pixels[offset + 3] * weight;
            alpha += sampleAlpha;
            for (let channel = 0; channel < 3; channel++)
                premultiplied[channel] += source.pixels[offset + channel] * sampleAlpha;
        }
        return alpha === 0 ? [0, 0, 0, 0] :
            [premultiplied[0] / alpha, premultiplied[1] / alpha,
                premultiplied[2] / alpha, alpha];
    };
    const resampleCanvasBitmap = (source, width, height, smooth) => {
        if (!Number.isInteger(width) || !Number.isInteger(height) || width <= 0 || height <= 0 ||
            width * height > MAX_CANVAS_PIXELS)
            throw new DOMException('Image resize exceeds the bitmap budget', 'NotSupportedError');
        const pixels = new Uint8ClampedArray(width * height * 4);
        for (let row = 0; row < height; row++) for (let column = 0; column < width; column++) {
            const x = (column + 0.5) * source.width / width - 0.5;
            const y = (row + 0.5) * source.height / height - 0.5;
            pixels.set(sampleCanvasBitmap(source, x, y, smooth), (row * width + column) * 4);
        }
        return { width, height, pixels };
    };
    Object.defineProperties(CanvasRenderingContext2D.prototype, {
        imageSmoothingEnabled: {
            get() { return this.__imageSmoothingEnabled; },
            set(value) { this.__imageSmoothingEnabled = Boolean(value); }
        },
        imageSmoothingQuality: {
            get() { return this.__imageSmoothingQuality; },
            set(value) { if (['low', 'medium', 'high'].includes(value)) this.__imageSmoothingQuality = value; }
        }
    });
    const paintCanvasImage = function(source, ...coordinates) {
        if (![2, 4, 8].includes(coordinates.length))
            throw new TypeError('drawImage requires 3, 5, or 9 arguments');
        const image = imageSourceSnapshot(source);
        const values = coordinates.map(Number);
        if (!values.every(Number.isFinite)) return false;
        let sourceX = 0, sourceY = 0, sourceWidth = image.width, sourceHeight = image.height;
        let destinationX, destinationY, destinationWidth, destinationHeight;
        if (values.length === 2) {
            [destinationX, destinationY] = values;
            destinationWidth = image.width; destinationHeight = image.height;
        } else if (values.length === 4) {
            [destinationX, destinationY, destinationWidth, destinationHeight] = values;
        } else {
            [sourceX, sourceY, sourceWidth, sourceHeight,
                destinationX, destinationY, destinationWidth, destinationHeight] = values;
        }
        if (!sourceWidth || !sourceHeight || !destinationWidth || !destinationHeight) return false;
        if (sourceWidth < 0) { sourceX += sourceWidth; sourceWidth = -sourceWidth; }
        if (sourceHeight < 0) { sourceY += sourceHeight; sourceHeight = -sourceHeight; }
        if (destinationWidth < 0) { destinationX += destinationWidth; destinationWidth = -destinationWidth; }
        if (destinationHeight < 0) { destinationY += destinationHeight; destinationHeight = -destinationHeight; }
        const target = stateForCanvas(this.canvas);
        if (!target.pixels) throw new DOMException('Canvas bitmap exceeds the budget', 'NotSupportedError');
        const bounds = canvasTransformedBounds(this.__transform,
            destinationX, destinationY, destinationWidth, destinationHeight, target);
        const inverse = matrixInverse2D(this.__transform);
        if (!inverse) return false;
        // A valid image entirely outside the bitmap still has a transparent source
        // layer for whole-canvas Porter-Duff operators such as copy/source-in.
        if (!bounds) return true;
        const [left, top, right, bottom] = bounds;
        for (let row = top; row < bottom; row++) for (let column = left; column < right; column++) {
            if (!canvasClipAllows(this, column, row, target.width)) continue;
            const [paintX, paintY] = matrixPoint2D(inverse, column + 0.5, row + 0.5);
            if (paintX < destinationX || paintY < destinationY ||
                paintX >= destinationX + destinationWidth || paintY >= destinationY + destinationHeight)
                continue;
            const sampleX = sourceX + (paintX - destinationX) * sourceWidth / destinationWidth - 0.5;
            const sampleY = sourceY + (paintY - destinationY) * sourceHeight / destinationHeight - 0.5;
            if (sampleX < -0.5 || sampleY < -0.5 ||
                sampleX >= image.width - 0.5 || sampleY >= image.height - 0.5) continue;
            const pixel = sampleCanvasBitmap(image, sampleX, sampleY, this.__imageSmoothingEnabled);
            compositeCanvasPixel(target.pixels, (row * target.width + column) * 4,
                pixel, this.__globalAlpha, this.__compositeOperation);
        }
        return true;
    };
    CanvasRenderingContext2D.prototype.drawImage = function(source, ...coordinates) {
        paintCanvasImage.call(this, source, ...coordinates);
    };
