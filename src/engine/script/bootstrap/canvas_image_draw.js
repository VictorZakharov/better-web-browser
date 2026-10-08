    // Shared bounded image sampling for ImageBitmap resize and Canvas drawImage.
    const canvasImagePaintHost = __hostCall;
    const canvasImagePaintEncode = canvasPrivateWireStringify;
    const canvasImageDrawArguments = (context, args) => {
        canvasImageDataContext(context);
        const count = canvasPrivateMath.min(args.length, 9);
        if (![3, 5, 9].includes(count))
            throw new TypeError('drawImage requires 3, 5, or 9 arguments');
        if (!canvasImageSourceSupported(args[0]))
            throw new TypeError('Unsupported Canvas image source');
        // Web IDL converts arguments before entering the drawing algorithm.
        // Unary + implements ToNumber (including rejecting Symbol/BigInt).
        // Surplus arguments after the longest overload are never inspected.
        const converted = [args[0]];
        for (let index = 1; index < count; index++) converted.push(+args[index]);
        return converted;
    };
    const sampleCanvasBitmap = (source, x, y, smooth) => {
        if (!smooth) {
            const column = canvasPrivateMath.max(0, canvasPrivateMath.min(source.width - 1, canvasPrivateMath.round(x)));
            const row = canvasPrivateMath.max(0, canvasPrivateMath.min(source.height - 1, canvasPrivateMath.round(y)));
            return canvasPrivateView(source.pixels, (row * source.width + column) * 4,
                (row * source.width + column) * 4 + 4);
        }
        const left = canvasPrivateMath.floor(x), top = canvasPrivateMath.floor(y);
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
            const clampedX = canvasPrivateMath.max(0, canvasPrivateMath.min(source.width - 1, column));
            const clampedY = canvasPrivateMath.max(0, canvasPrivateMath.min(source.height - 1, row));
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
    const resampleCanvasBitmap = (source, width, height, smooth, precise = false) => {
        if (!Number.isInteger(width) || !Number.isInteger(height) || width <= 0 || height <= 0 ||
            width * height > MAX_CANVAS_PIXELS)
            throw new DOMException('Image resize exceeds the bitmap budget', 'NotSupportedError');
        if (!precise) {
            const resized = canvasImagePaintHost('canvasResizeImage', canvasImagePaintEncode({
                width, height, source_width:source.width, source_height:source.height, smooth
            }), source.pixels);
            if (!resized || canvasPrivateByteLength(resized) !== width * height * 4)
                throw new DOMException('Image resize exceeds the native working budget', 'NotSupportedError');
            const pixels = new canvasPrivatePixelArray(canvasPrivateBuffer(resized),
                canvasPrivateOffset(resized), canvasPrivateCount(resized));
            return {width, height, pixels};
        }
        const pixels = precise ? new preciseBitmapWords(width * height * 4) : new canvasPrivatePixelArray(width * height * 4);
        for (let row = 0; row < height; row++) for (let column = 0; column < width; column++) {
            const x = (column + 0.5) * source.width / width - 0.5;
            const y = (row + 0.5) * source.height / height - 0.5;
            const sample=sampleCanvasBitmap(source,x,y,smooth);
            const offset = (row * width + column) * 4;
            for (let channel = 0; channel < 4; channel++)
                pixels[offset + channel] = precise ? canvasPrivateMath.round(sample[channel]) : sample[channel];
        }
        return { width, height, pixels };
    };
    defineCanvasContextProperties(CanvasRenderingContext2D.prototype, {
        imageSmoothingEnabled: {
            get() { return canvasDrawingState(this).imageSmoothingEnabled; },
            set(value) { canvasDrawingState(this).imageSmoothingEnabled = Boolean(value); }
        },
        imageSmoothingQuality: {
            get() { return canvasDrawingState(this).imageSmoothingQuality; },
            set(value) {
                value = `${value}`;
                if (['low', 'medium', 'high'].includes(value)) canvasDrawingState(this).imageSmoothingQuality = value;
            }
        }
    });
    const paintCanvasImage = function(source, ...coordinates) {
        if (![2, 4, 8].includes(coordinates.length))
            throw new TypeError('drawImage requires 3, 5, or 9 arguments');
        const values = coordinates.map(Number);
        if (!values.every(Number.isFinite)) return false;
        if (!canvasImageSourceUsable(source)) return false;
        const image = imageSourceSnapshot(source, false, true);
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
        const bounds = canvasTransformedBounds(canvasDrawingState(this).transform,
            destinationX, destinationY, destinationWidth, destinationHeight, target);
        const inverse = matrixInverse2D(canvasDrawingState(this).transform);
        if (!inverse) return false;
        if (image.originClean === false) target.originClean = false;
        // A valid image entirely outside the bitmap still has a transparent source
        // layer for whole-canvas Porter-Duff operators such as copy/source-in.
        if (!bounds) return true;
        const [left, top, right, bottom] = bounds;
        if (canvasPixelTag(image.pixels) === 'Uint8ClampedArray' &&
            (right - left) * (bottom - top) >= 256 && left <= right && top <= bottom) {
            const painted = canvasImagePaintHost('canvasPaintImage', canvasImagePaintEncode({
                width: target.width, height: target.height,
                source_width: image.width, source_height: image.height,
                bounds, inverse,
                source: [sourceX, sourceY, sourceWidth, sourceHeight],
                destination: [destinationX, destinationY, destinationWidth, destinationHeight],
                opacity: canvasDrawingState(this).globalAlpha, smooth: canvasDrawingState(this).imageSmoothingEnabled,
                operator: canvasDrawingState(this).compositeOperation
            }), target.pixels, image.pixels, canvasDrawingState(this).clipBits || null);
            if (painted && canvasPixelLength(painted) === canvasPrivateCount(target.pixels)) {
                copyCanvasPixelRow(target.pixels, 0, painted, 0, canvasPrivateCount(target.pixels));
                return true;
            }
        }
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
            const pixel = sampleCanvasBitmap(image, sampleX, sampleY, canvasDrawingState(this).imageSmoothingEnabled);
            compositeCanvasPixel(target.pixels, (row * target.width + column) * 4,
                pixel, canvasDrawingState(this).globalAlpha, canvasDrawingState(this).compositeOperation);
        }
        return true;
    };
    CanvasRenderingContext2D.prototype.drawImage = function(source, ...coordinates) {
        paintCanvasImage.apply(this, canvasImageDrawArguments(this, [source, ...coordinates]));
    };
