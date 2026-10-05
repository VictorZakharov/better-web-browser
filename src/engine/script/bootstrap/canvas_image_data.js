    // Raw pixel operations bypass transforms, compositing and clipping paths.
    // https://html.spec.whatwg.org/multipage/canvas.html#pixel-manipulation
    const canvasPixelArray = Uint8ClampedArray;
    const canvasPixelSet = Function.call.bind(canvasPixelArray.prototype.set);
    const canvasTypedPrototype = Object.getPrototypeOf(canvasPixelArray.prototype);
    const canvasPixelBuffer = Function.call.bind(Object.getOwnPropertyDescriptor(canvasTypedPrototype, 'buffer').get);
    const canvasPixelOffset = Function.call.bind(Object.getOwnPropertyDescriptor(canvasTypedPrototype, 'byteOffset').get);
    const canvasPixelLength = Function.call.bind(Object.getOwnPropertyDescriptor(canvasTypedPrototype, 'byteLength').get);
    const canvasImageDataContext = context => {
        const canvas = canvas2dOwners.get(context);
        if (!canvas) throw new TypeError('Pixel operation requires a CanvasRenderingContext2D');
        return canvas;
    };
    const canvasPixelInteger = value => {
        const number = Math.trunc(+value);
        if (!Number.isFinite(number) || number < -2147483648 || number > 2147483647)
            throw new TypeError('ImageData coordinates must fit an EnforceRange long');
        return number;
    };
    const copyCanvasPixelRow = (destination, destinationStart, source, sourceStart, length) => {
        const row = new canvasPixelArray(canvasPixelBuffer(source), canvasPixelOffset(source) + sourceStart, length);
        canvasPixelSet(destination, row, destinationStart);
    };

    const readCanvasImageData = (context, x, y, width, height, settings) => {
        const canvas = canvasImageDataContext(context);
        const rect = normalizedRectangle(canvasPixelInteger(x), canvasPixelInteger(y),
            canvasPixelInteger(width), canvasPixelInteger(height));
        if (!rect || rect.width === 0 || rect.height === 0)
            throw new DOMException('ImageData dimensions must be non-zero', 'IndexSizeError');
        const state = stateForCanvas(canvas);
        if (!state.pixels || rect.width * rect.height > MAX_CANVAS_PIXELS)
            throw new DOMException('The requested bitmap exceeds the canvas budget', 'NotSupportedError');
        const result = new ImageData(rect.width, rect.height, settings);
        const left = Math.max(0, rect.x), right = Math.min(state.width, rect.x + rect.width);
        const top = Math.max(0, rect.y), bottom = Math.min(state.height, rect.y + rect.height);
        if (right <= left || bottom <= top) return result;
        const length = (right - left) * 4;
        for (let row = top; row < bottom; row++) {
            copyCanvasPixelRow(result.data, ((row - rect.y) * rect.width + left - rect.x) * 4,
                state.pixels, (row * state.width + left) * 4, length);
        }
        return result;
    };

    const writeCanvasImageData = (context, imageData, x, y, dirty) => {
        const canvas = canvasImageDataContext(context);
        const image = imageDataStates.get(imageData);
        if (!image) throw new TypeError('putImageData requires ImageData');
        x = canvasPixelInteger(x); y = canvasPixelInteger(y);
        let [dirtyX, dirtyY, dirtyWidth, dirtyHeight] = dirty.length < 4
            ? [0, 0, image.width, image.height] : dirty.slice(0,4).map(canvasPixelInteger);
        if (canvasPixelLength(image.data) !== image.width * image.height * 4)
            throw new DOMException('ImageData storage is detached or out of bounds', 'InvalidStateError');
        const state = stateForCanvas(canvas);
        if (!state.pixels) return;
        if (dirtyWidth < 0) { dirtyX += dirtyWidth; dirtyWidth = -dirtyWidth; }
        if (dirtyHeight < 0) { dirtyY += dirtyHeight; dirtyHeight = -dirtyHeight; }
        // Intersect in source coordinates before copying. One typed-array copy
        // per surviving row replaces a temporary view allocation per pixel.
        const left = Math.max(0, dirtyX, -x), top = Math.max(0, dirtyY, -y);
        const right = Math.min(image.width, dirtyX + dirtyWidth, state.width - x);
        const bottom = Math.min(image.height, dirtyY + dirtyHeight, state.height - y);
        if (right <= left || bottom <= top) return;
        const length = (right - left) * 4;
        for (let row = top; row < bottom; row++) {
            copyCanvasPixelRow(state.pixels, ((y + row) * state.width + x + left) * 4,
                image.data, (row * image.width + left) * 4, length);
        }
    };
