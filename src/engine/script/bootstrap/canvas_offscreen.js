    // OffscreenCanvas reuses the bounded 2D bitmap without manufacturing a DOM element.
    const offscreenContextToken = Symbol('OffscreenCanvasRenderingContext2D');
    const canvasUnsignedDimension = value => {
        // Web IDL [EnforceRange] unsigned long long: convert before truncating,
        // reject BigInt/Symbol, and preserve the full exactly representable range.
        value = +value;
        if (!Number.isFinite(value))
            throw new TypeError('Canvas dimensions must be non-negative finite integers');
        value = canvasPrivateMath.trunc(value);
        if (value < 0 || value > 9007199254740991)
            throw new TypeError('Canvas dimension is outside the unsigned integer range');
        return value === 0 ? 0 : value;
    };
    class OffscreenCanvasRenderingContext2D extends CanvasRenderingContext2D {
        constructor(token, canvas, settings) {
            if (token !== offscreenContextToken) throw new TypeError('Illegal constructor');
            super(canvas, canvas2dContextToken, settings);
        }
    }
    // CanvasPathDrawingStyles is included by both context interfaces. Its IDL
    // members belong to each prototype, even while painting shares the base.
    for(const name of ['lineWidth','miterLimit','lineDashOffset','lineCap','lineJoin','setLineDash','getLineDash'])
        Object.defineProperty(OffscreenCanvasRenderingContext2D.prototype,name,
            Object.getOwnPropertyDescriptor(CanvasRenderingContext2D.prototype,name));
    const offscreenCanvasBrands = new WeakSet();
    class OffscreenCanvas extends EventTarget {
        constructor(width, height) {
            super();
            if (arguments.length < 2) throw new TypeError('OffscreenCanvas requires width and height');
            canvasOwnerWeakSet(offscreenCanvasDimensions, this, {
                width: canvasUnsignedDimension(width), height: canvasUnsignedDimension(height), detached: false
            });
            offscreenCanvasBrands.add(this);
            stateForCanvas(this);
        }
        get width() { const state=canvasOffscreenDimensions(this);return state.detached ? 0 : state.width; }
        set width(value) {
            const state=canvasOffscreenDimensions(this);
            const width=canvasUnsignedDimension(value);
            if (state.detached) throw new DOMException('OffscreenCanvas is detached', 'InvalidStateError');
            state.width = width;
            stateForCanvas(this, true);
        }
        get height() { const state=canvasOffscreenDimensions(this);return state.detached ? 0 : state.height; }
        set height(value) {
            const state=canvasOffscreenDimensions(this);
            const height=canvasUnsignedDimension(value);
            if (state.detached) throw new DOMException('OffscreenCanvas is detached', 'InvalidStateError');
            state.height = height;
            stateForCanvas(this, true);
        }
        getContext(contextId, options = undefined) {
            const dimensions = canvasOffscreenDimensions(this);
            if (!arguments.length) throw new TypeError('getContext requires a context identifier');
            const requested = `${contextId}`;
            if (!['2d', 'bitmaprenderer', 'webgl', 'webgl2', 'webgpu'].includes(requested))
                throw new TypeError('Invalid OffscreenRenderingContextId');
            if (dimensions.detached) throw new DOMException('OffscreenCanvas is detached', 'InvalidStateError');
            const mode = requested === 'webgl2' ? 'webgl' : requested;
            if (!['2d', 'bitmaprenderer', 'webgl'].includes(mode)) return null;
            const state = stateForCanvas(this);
            if (state.mode !== 'none' && state.mode !== mode) return null;
            if (state.context) return mode === 'webgl' &&
                webGlState(state.context).api !== (requested === 'webgl2' ? 'webgl2' : 'webgl1') ? null : state.context;
            const settings = mode === '2d' ? canvasConvertSettings(options) : null;
            if (settings && !canvasSettingsSupported(settings)) return null;
            const context = mode === 'webgl' ? createWebGlContext(this, options, requested === 'webgl2' ? 'webgl2' : 'webgl1') : mode === '2d' ?
                new OffscreenCanvasRenderingContext2D(offscreenContextToken, this, settings) :
                new ImageBitmapRenderingContext(canvasBitmapContextToken, this, options);
            if (!context) return null;
            state.context = context;
            state.mode = mode;
            return context;
        }
        convertToBlob(options = {}) {
            if (canvasOffscreenDimensions(this).detached)
                return Promise.reject(new DOMException('OffscreenCanvas is detached', 'InvalidStateError'));
            const [width,height]=canvasOwnedDimensions(this);
            if (!width || !height)
                return Promise.reject(new DOMException('Canvas has no pixels', 'IndexSizeError'));
            try {
                const encoded = encodedCanvas(this, String(options?.type ?? 'image/png'), options?.quality);
                if (!encoded) throw new DOMException('Canvas could not be encoded', 'EncodingError');
                const blob = new Blob([encoded[1]], { type: encoded[0] });
                return Promise.resolve().then(() => blob);
            } catch (error) { return Promise.reject(error); }
        }
        transferToImageBitmap() {
            if (canvasOffscreenDimensions(this).detached) throw new DOMException('OffscreenCanvas is detached', 'InvalidStateError');
            const state = stateForCanvas(this);
            if (state.mode === 'none')
                throw new DOMException('OffscreenCanvas has no rendering context', 'InvalidStateError');
            if (!state.pixels || !state.width || !state.height)
                throw new DOMException('Canvas has no available bitmap', 'InvalidStateError');
            const result = makeImageBitmap(state.width, state.height, state.pixels, false, null, state.originClean !== false);
            state.pixels = new canvasPrivatePixelArray(state.width * state.height * 4);
            state.originClean = true;
            if (state.mode === '2d') canvasInitializeOutputBitmap(state.context, state.pixels);
            if (state.mode === 'webgl') resetWebGlCanvas(state);
            if (state.mode === 'bitmaprenderer') resetCanvasBitmapRenderer(state.context);
            return result;
        }
    }
    HTMLCanvasElement.prototype.transferControlToOffscreen = function() {
        canvasHtmlReceiver(this);
        const state = stateForCanvas(this);
        if (state.mode !== 'none')
            throw new DOMException('Canvas already has a rendering context', 'InvalidStateError');
        const offscreen = new OffscreenCanvas(...canvasOwnedDimensions(this));
        state.mode = 'placeholder';
        state.placeholder = offscreen;
        return offscreen;
    };
    // HTML structured serialize/deserialize with transfer. Pixel storage is
    // copied into the message record before the sender is detached; a cloned
    // ImageBitmap stays usable, while OffscreenCanvas requires transfer.
    globalThis.__cloneCanvasBindings = {
        isBitmap: value => canvasPrivateWeakHas(imageBitmapStates, value),
        isOffscreen: value => canvasSourceSetHas(offscreenCanvasBrands,value),
        isDetached: value => canvasSourceSetHas(offscreenCanvasBrands,value) ? canvasOffscreenDetached(value) :
            !canvasPrivateWeakGet(imageBitmapStates, value)?.pixels,
        snapshot(value) {
            if (canvasSourceSetHas(offscreenCanvasBrands,value)) {
                if (canvasOffscreenDetached(value)) throw new DOMException('Canvas is detached', 'DataCloneError');
                const state = stateForCanvas(value);
                // HTML transfer steps require context mode "none", including
                // 2D and bitmaprenderer. Transfer an ImageBitmap for painted pixels.
                if (state.mode !== 'none') throw new DOMException('A Canvas with an active rendering context cannot be transferred', 'InvalidStateError');
                if (!state.pixels) throw new DOMException('Canvas exceeds the bitmap budget', 'DataCloneError');
                return { width: state.width, height: state.height, pixels: state.pixels,
                    canvasWidth: canvasOwnedDimensions(value)[0], canvasHeight: canvasOwnedDimensions(value)[1],
                    alpha: canvasPrivateWeakGet(bitmapRendererStates, state.context)?.alpha,
                    mode: state.mode, kind: 'offscreencanvas' };
            }
            const state = imageBitmapPixels(value);
            // HTML ImageBitmap serialization and transfer reject tainted data
            // before exposing bytes to the clone wire or detaching the sender.
            if (state.originClean === false)
                throw new DOMException('ImageBitmap is not origin-clean', 'DataCloneError');
            bitmapPrecisionBudget(state,state.width,state.height);
            return { width: state.width, height: state.height, pixels: state.pixels,
                precise:encodedBitmapWords(state.pixels16),
                premultiplied: state.premultiplied, kind: 'imagebitmap' };
        },
        detach(value) {
            if (canvasSourceSetHas(offscreenCanvasBrands,value)) {
                canvasOffscreenDimensions(value).detached = true;
                const state = canvasPrivateWeakGet(canvasStates, value);
                if (state) { state.pixels = null; state.context = null; }
            } else closeImageBitmap(value);
        },
        receive(record, bytes, preciseBytes = null) {
            const width = Number(record.w), height = Number(record.h);
            if (!Number.isInteger(width) || !Number.isInteger(height) || width < 0 || height < 0 ||
                width * height > MAX_CANVAS_PIXELS || bytes.length !== width * height * 4)
                throw new DOMException('Invalid bitmap transfer', 'DataCloneError');
            const pixels = new canvasPrivatePixelArray(bytes);
            if (preciseBytes && (record.t !== 'imagebitmap' || preciseBytes.byteLength!==width*height*8))
                throw new DOMException('Invalid precise bitmap transfer','DataCloneError');
            if (record.t === 'imagebitmap') return makeImageBitmap(width, height, pixels,
                record.a === true,decodedBitmapWords(preciseBytes));
            if (record.t !== 'offscreencanvas')
                throw new DOMException('Unknown bitmap transfer', 'DataCloneError');
            if (record.m !== 'none')
                throw new DOMException('Invalid Canvas context mode in transfer', 'DataCloneError');
            const canvasWidth = record.cw ?? width, canvasHeight = record.ch ?? height;
            if (!Number.isInteger(canvasWidth) || !Number.isInteger(canvasHeight) ||
                canvasWidth < 0 || canvasHeight < 0 || canvasWidth > 9007199254740991 || canvasHeight > 9007199254740991)
                throw new DOMException('Invalid Canvas dimensions in transfer', 'DataCloneError');
            const canvas = new OffscreenCanvas(canvasWidth, canvasHeight), state = stateForCanvas(canvas);
            state.width = width; state.height = height;
            state.pixels = pixels;
            return canvas;
        }
    };
