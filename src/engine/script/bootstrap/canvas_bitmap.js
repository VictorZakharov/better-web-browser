    // HTML Canvas: ImageBitmap owns a transferable, closeable snapshot of straight-alpha RGBA.
    const imageBitmapToken = Symbol('ImageBitmap');
    const canvasBitmapContextToken = Symbol('ImageBitmapRenderingContext');
    const imageBitmapStates = new WeakMap();
    class ImageBitmap {
        constructor(token, width, height, pixels) {
            if (token !== imageBitmapToken) throw new TypeError('Illegal constructor');
            imageBitmapStates.set(this, { width, height, pixels });
        }
        get width() { return imageBitmapStates.get(this)?.pixels ? imageBitmapStates.get(this).width : 0; }
        get height() { return imageBitmapStates.get(this)?.pixels ? imageBitmapStates.get(this).height : 0; }
        close() {
            const state = imageBitmapStates.get(this);
            if (state) { state.width = 0; state.height = 0; state.pixels = null; }
        }
    }
    const imageBitmapPixels = bitmap => {
        const state = imageBitmapStates.get(bitmap);
        if (!state?.pixels) throw new DOMException('ImageBitmap is closed', 'InvalidStateError');
        return state;
    };
    const makeImageBitmap = (width, height, pixels) => {
        if (!Number.isInteger(width) || !Number.isInteger(height) || width <= 0 || height <= 0 ||
            width * height > MAX_CANVAS_PIXELS || pixels.length !== width * height * 4)
            throw new DOMException('ImageBitmap exceeds the bitmap budget', 'NotSupportedError');
        return new ImageBitmap(imageBitmapToken, width, height, pixels);
    };
    const canvasBitmapSnapshot = canvas => {
        const state = stateForCanvas(canvas);
        const output = state.placeholder ? stateForCanvas(state.placeholder) : state;
        if (!output.pixels || !output.width || !output.height)
            throw new DOMException('Canvas has no available bitmap', 'InvalidStateError');
        return { width: output.width, height: output.height, pixels: new Uint8ClampedArray(output.pixels) };
    };
    const imageSourceSnapshot = (source, allowImageData = false) => {
        if (source instanceof ImageBitmap) {
            const { width, height, pixels } = imageBitmapPixels(source);
            return { width, height, pixels: new Uint8ClampedArray(pixels) };
        }
        if (source instanceof HTMLCanvasElement || source instanceof OffscreenCanvas)
            return canvasBitmapSnapshot(source);
        if (source instanceof HTMLImageElement) {
            // Only decoded, same-origin/CORS-readable bytes may enter Canvas.
            // Opaque image responses remain inaccessible through this path.
            const decoded = detachedImageLoads.get(source)?.decoded;
            if (!decoded || !source.complete)
                throw new DOMException('Image has no available bitmap', 'InvalidStateError');
            return { width: decoded.width, height: decoded.height,
                pixels: new Uint8ClampedArray(decoded.pixels) };
        }
        if (allowImageData && source instanceof ImageData)
            return { width: source.width, height: source.height, pixels: new Uint8ClampedArray(source.data) };
        throw new TypeError('Unsupported Canvas image source');
    };
    const cropImageBitmap = (source, x, y, width, height) => {
        [x, y, width, height] = [x, y, width, height].map(Number);
        if (![x, y, width, height].every(Number.isFinite) || width === 0 || height === 0)
            throw new DOMException('Invalid ImageBitmap crop rectangle', 'IndexSizeError');
        if (width < 0) { x += width; width = -width; }
        if (height < 0) { y += height; height = -height; }
        x = Math.floor(x); y = Math.floor(y); width = Math.ceil(width); height = Math.ceil(height);
        if (width * height > MAX_CANVAS_PIXELS)
            throw new DOMException('ImageBitmap crop exceeds the bitmap budget', 'NotSupportedError');
        const pixels = new Uint8ClampedArray(width * height * 4);
        for (let row = 0; row < height; row++) for (let column = 0; column < width; column++) {
            const fromX = x + column, fromY = y + row;
            if (fromX < 0 || fromY < 0 || fromX >= source.width || fromY >= source.height) continue;
            const offset = (row * width + column) * 4;
            pixels.set(source.pixels.subarray((fromY * source.width + fromX) * 4,
                (fromY * source.width + fromX) * 4 + 4), offset);
        }
        return { width, height, pixels };
    };
    const imageBitmapOptions = (source, options) => {
        if (options === null || typeof options !== 'object') throw new TypeError('ImageBitmap options must be an object');
        const orientation = options.imageOrientation ?? 'from-image';
        if (!['from-image', 'flipY', 'none'].includes(orientation))
            throw new TypeError('Invalid ImageBitmap imageOrientation');
        const quality = options.resizeQuality ?? 'low';
        if (!['pixelated', 'low', 'medium', 'high'].includes(quality))
            throw new TypeError('Invalid ImageBitmap resizeQuality');
        const width = options.resizeWidth === undefined ? undefined : Number(options.resizeWidth);
        const height = options.resizeHeight === undefined ? undefined : Number(options.resizeHeight);
        if ((width !== undefined && (!Number.isInteger(width) || width <= 0)) ||
            (height !== undefined && (!Number.isInteger(height) || height <= 0)))
            throw new DOMException('ImageBitmap resize dimensions must be positive', 'InvalidStateError');
        const outputWidth = width ?? Math.max(1, Math.round(source.width * (height ?? source.height) / source.height));
        const outputHeight = height ?? Math.max(1, Math.round(source.height * outputWidth / source.width));
        if (outputWidth * outputHeight > MAX_CANVAS_PIXELS)
            throw new DOMException('ImageBitmap resize exceeds the bitmap budget', 'NotSupportedError');
        let output = outputWidth === source.width && outputHeight === source.height ? source :
            resampleCanvasBitmap(source, outputWidth, outputHeight, quality !== 'pixelated');
        if (orientation === 'flipY') {
            const pixels = new Uint8ClampedArray(output.pixels.length);
            const stride = output.width * 4;
            for (let row = 0; row < output.height; row++)
                pixels.set(output.pixels.subarray(row * stride, (row + 1) * stride),
                    (output.height - row - 1) * stride);
            output = { width: output.width, height: output.height, pixels };
        }
        return output;
    };
    globalThis.createImageBitmap = (source, ...arguments_) => {
        try {
            const cropped = arguments_.length >= 4;
            if (arguments_.length !== 0 && arguments_.length !== 1 &&
                arguments_.length !== 4 && arguments_.length !== 5)
                throw new TypeError('createImageBitmap requires a source, optional crop, and options');
            const options = cropped ? arguments_[4] ?? {} : arguments_[0] ?? {};
            const finish = snapshot => {
                const region = cropped ? cropImageBitmap(snapshot, ...arguments_.slice(0, 4)) : snapshot;
                const output = imageBitmapOptions(region, options);
                return makeImageBitmap(output.width, output.height, output.pixels);
            };
            if (source instanceof Blob) return source.bytes().then(bytes => {
                const decoded = host('canvasDecode', bytes);
                if (!decoded) throw new DOMException('Image could not be decoded', 'InvalidStateError');
                return finish({ width: decoded[0], height: decoded[1], pixels: decoded[2] });
            });
            const snapshot = imageSourceSnapshot(source, true);
            return Promise.resolve().then(() => finish(snapshot));
        } catch (error) { return Promise.reject(error); }
    };
    class ImageBitmapRenderingContext {
        constructor(token, canvas) {
            if (token !== canvasBitmapContextToken) throw new TypeError('Illegal constructor');
            Object.defineProperty(this, 'canvas', { enumerable: true, value: canvas });
        }
        __reset() {}
        transferFromImageBitmap(bitmap) {
            const state = stateForCanvas(this.canvas);
            if (bitmap === null) {
                if (state.pixels) state.pixels.fill(0);
                return;
            }
            if (!(bitmap instanceof ImageBitmap)) throw new TypeError('Expected ImageBitmap or null');
            const image = imageBitmapPixels(bitmap);
            if (!state.pixels) throw new DOMException('Canvas bitmap exceeds the budget', 'NotSupportedError');
            if (image.width === state.width && image.height === state.height) state.pixels = image.pixels;
            else state.pixels = resampleCanvasBitmap(image, state.width, state.height, true).pixels;
            bitmap.close();
        }
    }
