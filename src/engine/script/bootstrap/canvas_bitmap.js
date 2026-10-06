    // HTML Canvas: ImageBitmap owns a transferable, closeable snapshot of straight-alpha RGBA.
    const imageBitmapToken = Symbol('ImageBitmap');
    const canvasBitmapContextToken = Symbol('ImageBitmapRenderingContext');
    const imageBitmapStates = new WeakMap();
    let readImageBitmapBlob = globalThis.__imageBitmapBlobSnapshot;
    delete globalThis.__imageBitmapBlobSnapshot;
    if (!readImageBitmapBlob) globalThis.__bindImageBitmapBlob = reader => { readImageBitmapBlob = reader; };
    const imageBitmapState = bitmap => {
        const state = imageBitmapStates.get(bitmap);
        if (!state) throw new TypeError('Illegal ImageBitmap receiver');
        return state;
    };
    const closeImageBitmap = bitmap => {
        const state = imageBitmapState(bitmap);
        state.width = 0; state.height = 0; state.pixels = null; state.pixels16 = null;
    };
    class ImageBitmap {
        constructor(token, width, height, pixels, premultiplied, pixels16) {
            if (token !== imageBitmapToken) throw new TypeError('Illegal constructor');
            imageBitmapStates.set(this, { width, height, pixels, premultiplied, pixels16 });
        }
        get width() { const state = imageBitmapState(this); return state.pixels ? state.width : 0; }
        get height() { const state = imageBitmapState(this); return state.pixels ? state.height : 0; }
        close() { closeImageBitmap(this); }
    }
    const imageBitmapPixels = bitmap => {
        const state = imageBitmapState(bitmap);
        if (!state.pixels) throw new DOMException('ImageBitmap is closed', 'InvalidStateError');
        return state;
    };
    const makeImageBitmap = (width, height, pixels, premultiplied = false, pixels16 = null) => {
        if (!Number.isInteger(width) || !Number.isInteger(height) || width <= 0 || height <= 0 ||
            width * height > MAX_CANVAS_PIXELS || pixels.length !== width * height * 4 ||
            pixels16 && pixels16.length !== pixels.length)
            throw new DOMException('ImageBitmap exceeds the bitmap budget', 'NotSupportedError');
        return new ImageBitmap(imageBitmapToken, width, height, pixels, premultiplied, pixels16);
    };
    const canvasBitmapSnapshot = canvas => {
        const state = stateForCanvas(canvas);
        const output = state.placeholder ? stateForCanvas(state.placeholder) : state;
        if (!output.pixels || !output.width || !output.height)
            throw new DOMException('Canvas has no available bitmap', 'InvalidStateError');
        return { width: output.width, height: output.height, pixels: new Uint8ClampedArray(output.pixels) };
    };
    const canvasSourceWeakGet = Function.call.bind(WeakMap.prototype.get);
    const canvasSourceWeakHas = Function.call.bind(WeakMap.prototype.has);
    const canvasSourceSetHas = Function.call.bind(WeakSet.prototype.has);
    const canvasSourceElement = (source, localName) => {
        return canvasNativeElement(source, localName);
    };
    const canvasImageSourceSupported = source => canvasSourceWeakHas(videoFrameStates, source) ||
        canvasSourceWeakHas(imageBitmapStates, source) ||
        canvasSourceSetHas(offscreenCanvasBrands, source) ||
        canvasSourceElement(source, 'canvas') || canvasSourceElement(source, 'img');
    const canvasImageSourceUsable = source => {
        // HTML distinguishes an incomplete image (no drawing) from a broken
        // request (InvalidStateError). Author properties cannot supply pixels.
        // https://html.spec.whatwg.org/multipage/canvas.html#check-the-usability-of-the-image-argument
        if (!canvasSourceElement(source, 'img')) return true;
        const image = imageElementState(source);
        if (image.broken) throw new DOMException('Image request is broken', 'InvalidStateError');
        const load = canvasSourceWeakGet(detachedImageLoads, source);
        return image.naturalWidth > 0 && image.naturalHeight > 0 &&
            (load?.source === image.source && !!load.decoded ||
                !!__hostCall('imageElementMetadata', canvasOwnerWeakGet(nodeHandles, source)));
    };
    const imageSourceSnapshot = (source, allowImageData = false) => {
        if (videoFrameStates.has(source)) return videoFrameSnapshot(source);
        if (imageBitmapStates.has(source)) {
            const state = imageBitmapPixels(source);
            const pixels16=copyBitmapWords(state);
            return { width: state.width, height: state.height,
                pixels: pixels16?narrowBitmapWords(pixels16):bitmapStraightPixels(state), pixels16 };
        }
        if (canvasSourceSetHas(offscreenCanvasBrands, source) || canvasSourceElement(source, 'canvas'))
            return canvasBitmapSnapshot(source);
        if (canvasSourceElement(source, 'img')) {
            // Only decoded, same-origin/CORS-readable bytes may enter Canvas.
            // Opaque image responses remain inaccessible through this path.
            const load = canvasSourceWeakGet(detachedImageLoads, source);
            const decoded = load?.source === imageElementState(source).source ? load.decoded : null;
            if (decoded) return { width: decoded.width, height: decoded.height,
                pixels: new Uint8ClampedArray(decoded.pixels), pixels16: decoded.pixels16 };
            const owned = __hostCall('imageElementBitmap', canvasOwnerWeakGet(nodeHandles, source));
            if (owned === 'tainted')
                throw new DOMException('Image pixels are not origin-clean', 'SecurityError');
            if (!owned) throw new DOMException('Image has no available bitmap', 'InvalidStateError');
            return {width:owned[0], height:owned[1], pixels:new Uint8ClampedArray(owned[2])};
        }
        if (allowImageData && imageDataStates.has(source)) {
            const state=imageDataStates.get(source);
            if (canvasPixelLength(state.data) !== state.width * state.height * 4)
                throw new DOMException('ImageData buffer is detached', 'InvalidStateError');
            return { width: state.width, height: state.height, pixels: new canvasPixelArray(state.data) };
        }
        throw new TypeError('Unsupported Canvas image source');
    };
    const cropImageBitmap = (source, x, y, width, height) => {
        if (width === 0 || height === 0) throw new RangeError('ImageBitmap crop dimension is zero');
        if (width < 0) { x += width; width = -width; }
        if (height < 0) { y += height; height = -height; }
        bitmapPixelBudget(width, height);
        const pixels16=cropBitmapWords(source,x,y,width,height);
        const pixels = new Uint8ClampedArray(width * height * 4);
        for (let row = 0; row < height; row++) for (let column = 0; column < width; column++) {
            const fromX = x + column, fromY = y + row;
            if (fromX < 0 || fromY < 0 || fromX >= source.width || fromY >= source.height) continue;
            const offset = (row * width + column) * 4;
            pixels.set(source.pixels.subarray((fromY * source.width + fromX) * 4,
                (fromY * source.width + fromX) * 4 + 4), offset);
        }
        return { width, height, pixels:pixels16?narrowBitmapWords(pixels16):pixels, pixels16 };
    };
    globalThis.createImageBitmap = function createImageBitmap(source, ...arguments_) {
        if (arguments.length === 0) throw new TypeError('createImageBitmap requires an image source');
        const cropped = arguments_.length >= 4;
        if (arguments_.length !== 0 && arguments_.length !== 1 && arguments_.length < 4)
            throw new TypeError('createImageBitmap requires a source, optional crop, and options');
        // IDL conversion errors throw synchronously, before returning a Promise.
        const rectangle = cropped ? arguments_.slice(0, 4).map(bitmapLong) : null;
        const options = convertImageBitmapOptions(cropped ? arguments_[4] : arguments_[0]);
        try {
            if (rectangle && (!rectangle[2] || !rectangle[3]))
                throw new RangeError('ImageBitmap crop dimension is zero');
            if (options.resizeWidth === 0 || options.resizeHeight === 0)
                throw new DOMException('ImageBitmap resize dimension is zero', 'InvalidStateError');
            const finish = snapshot => {
                const region = rectangle ? cropImageBitmap(snapshot, ...rectangle) : snapshot;
                const output = formatImageBitmap(region, options);
                // No-transform creation still owns a snapshot, not the image's
                // decoder storage. Closing/transferring one bitmap cannot affect peers.
                bitmapPrecisionBudget(output,output.width,output.height);
                return makeImageBitmap(output.width, output.height, output.pixels, output.premultiplied,
                    output.pixels16?new preciseBitmapWords(output.pixels16):null);
            };
            const bytes = readImageBitmapBlob(source);
            if (bytes) return Promise.resolve().then(() => {
                const decoded = host('canvasDecode', bytes,
                    options.imageOrientation !== 'from-image', options.colorSpaceConversion === 'none', true);
                if (!decoded) throw new DOMException('Image could not be decoded', 'InvalidStateError');
                return finish({ width: decoded[0], height: decoded[1], pixels: decoded[2],
                    pixels16:decodedBitmapWords(decoded[3]) });
            });
            const snapshot = imageSourceSnapshot(source, true);
            return Promise.resolve().then(() => finish(snapshot));
        } catch (error) { return Promise.reject(error); }
    };
    const bitmapRendererStates = new WeakMap();
    const bitmapRendererState = context => {
        const state = bitmapRendererStates.get(context);
        if (!state) throw new TypeError('Illegal ImageBitmapRenderingContext receiver');
        return state;
    };
    const opaqueBitmap = pixels => {
        for (let offset = 0; offset < pixels.length; offset += 4) {
            const alpha = pixels[offset + 3];
            for (let channel = 0; channel < 3; channel++)
                pixels[offset + channel] = Math.floor((pixels[offset + channel] * alpha + 127) / 255);
            pixels[offset + 3] = 255;
        }
    };
    const resetCanvasBitmapRenderer = context => {
        const renderer = bitmapRendererState(context);
        renderer.blank = true;
        const state = canvasStates.get(renderer.canvas);
        if (!renderer.alpha && state?.pixels) opaqueBitmap(state.pixels);
    };
    class ImageBitmapRenderingContext {
        constructor(token, canvas, options) {
            if (token !== canvasBitmapContextToken) throw new TypeError('Illegal constructor');
            if (options === null || options === undefined) options = {};
            if (typeof options !== 'object' && typeof options !== 'function')
                throw new TypeError('Bitmap renderer settings must be a dictionary');
            const setting = options.alpha;
            bitmapRendererStates.set(this, {canvas, alpha: setting === undefined ? true : Boolean(setting)});
            resetCanvasBitmapRenderer(this);
        }
        get canvas() { return bitmapRendererState(this).canvas; }
        transferFromImageBitmap(bitmap) {
            const renderer = bitmapRendererState(this);
            if (canvasOffscreenDetached(renderer.canvas))
                throw new DOMException('Canvas is detached', 'InvalidStateError');
            if (bitmap === null) {
                stateForCanvas(renderer.canvas, true);
                return;
            }
            if (!imageBitmapStates.has(bitmap)) throw new TypeError('Expected ImageBitmap or null');
            const image = imageBitmapPixels(bitmap);
            const state = stateForCanvas(renderer.canvas);
            state.width = image.width;
            state.height = image.height;
            // Take ownership without resampling to the content-attribute size.
            // Straight inputs need no copy; associated inputs are normalized for
            // the Canvas representation before the source is detached.
            state.pixels = image.premultiplied ? bitmapStraightPixels(image) : image.pixels;
            renderer.blank = false;
            if (!renderer.alpha) opaqueBitmap(state.pixels);
            closeImageBitmap(bitmap);
        }
    }
