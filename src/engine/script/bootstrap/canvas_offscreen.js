    // OffscreenCanvas reuses the bounded 2D bitmap without manufacturing a DOM element.
    const offscreenContextToken = Symbol('OffscreenCanvasRenderingContext2D');
    const canvasUnsignedDimension = value => {
        value = Number(value);
        if (!Number.isFinite(value) || value < 0)
            throw new TypeError('Canvas dimensions must be non-negative finite integers');
        return Math.min(0xffffffff, Math.trunc(value));
    };
    class OffscreenCanvasRenderingContext2D extends CanvasRenderingContext2D {
        constructor(token, canvas) {
            if (token !== offscreenContextToken) throw new TypeError('Illegal constructor');
            super(canvas);
        }
    }
    const offscreenCanvasBrands = new WeakSet();
    class OffscreenCanvas extends EventTarget {
        constructor(width, height) {
            super();
            if (arguments.length < 2) throw new TypeError('OffscreenCanvas requires width and height');
            this.__width = canvasUnsignedDimension(width);
            this.__height = canvasUnsignedDimension(height);
            this.__detached = false;
            offscreenCanvasBrands.add(this);
            stateForCanvas(this);
        }
        get width() { return this.__detached ? 0 : this.__width; }
        set width(value) {
            if (this.__detached) throw new DOMException('OffscreenCanvas is detached', 'InvalidStateError');
            this.__width = canvasUnsignedDimension(value);
            stateForCanvas(this, true);
        }
        get height() { return this.__detached ? 0 : this.__height; }
        set height(value) {
            if (this.__detached) throw new DOMException('OffscreenCanvas is detached', 'InvalidStateError');
            this.__height = canvasUnsignedDimension(value);
            stateForCanvas(this, true);
        }
        getContext(contextId, options = undefined) {
            if (this.__detached) throw new DOMException('OffscreenCanvas is detached', 'InvalidStateError');
            const requested = String(contextId);
            const mode = ['experimental-webgl','webgl2'].includes(requested) ? 'webgl' : requested;
            if (!['2d', 'bitmaprenderer', 'webgl'].includes(mode)) return null;
            const state = stateForCanvas(this);
            if (state.mode !== 'none' && state.mode !== mode) return null;
            if (state.context) return mode === 'webgl' &&
                webGlState(state.context).api !== (requested === 'webgl2' ? 'webgl2' : 'webgl1') ? null : state.context;
            const context = mode === 'webgl' ? createWebGlContext(this, options, requested === 'webgl2' ? 'webgl2' : 'webgl1') : mode === '2d' ?
                new OffscreenCanvasRenderingContext2D(offscreenContextToken, this) :
                new ImageBitmapRenderingContext(canvasBitmapContextToken, this, options);
            if (!context) return null;
            state.context = context;
            state.mode = mode;
            return context;
        }
        convertToBlob(options = {}) {
            if (this.__detached)
                return Promise.reject(new DOMException('OffscreenCanvas is detached', 'InvalidStateError'));
            if (!this.width || !this.height)
                return Promise.reject(new DOMException('Canvas has no pixels', 'IndexSizeError'));
            try {
                const encoded = encodedCanvas(this, String(options?.type ?? 'image/png'), options?.quality);
                if (!encoded) throw new DOMException('Canvas could not be encoded', 'EncodingError');
                const blob = new Blob([encoded[1]], { type: encoded[0] });
                return Promise.resolve().then(() => blob);
            } catch (error) { return Promise.reject(error); }
        }
        transferToImageBitmap() {
            if (this.__detached) throw new DOMException('OffscreenCanvas is detached', 'InvalidStateError');
            const state = stateForCanvas(this);
            if (state.mode === 'none')
                throw new DOMException('OffscreenCanvas has no rendering context', 'InvalidStateError');
            if (!state.pixels || !state.width || !state.height)
                throw new DOMException('Canvas has no available bitmap', 'InvalidStateError');
            const result = makeImageBitmap(state.width, state.height, state.pixels);
            state.pixels = new Uint8ClampedArray(state.width * state.height * 4);
            if (state.mode === 'webgl') resetWebGlCanvas(state);
            if (state.mode === 'bitmaprenderer') resetCanvasBitmapRenderer(state.context);
            return result;
        }
    }
    HTMLCanvasElement.prototype.transferControlToOffscreen = function() {
        if (!(this instanceof HTMLCanvasElement)) throw new TypeError('Illegal canvas receiver');
        const state = stateForCanvas(this);
        if (state.mode !== 'none')
            throw new DOMException('Canvas already has a rendering context', 'InvalidStateError');
        const offscreen = new OffscreenCanvas(this.width, this.height);
        state.mode = 'placeholder';
        state.placeholder = offscreen;
        return offscreen;
    };
    // HTML structured serialize/deserialize with transfer. Pixel storage is
    // copied into the message record before the sender is detached; a cloned
    // ImageBitmap stays usable, while OffscreenCanvas requires transfer.
    globalThis.__cloneCanvasBindings = {
        isBitmap: value => imageBitmapStates.has(value),
        isOffscreen: value => value instanceof OffscreenCanvas,
        isDetached: value => value instanceof OffscreenCanvas ? value.__detached :
            !imageBitmapStates.get(value)?.pixels,
        snapshot(value) {
            if (value instanceof OffscreenCanvas) {
                if (value.__detached) throw new DOMException('Canvas is detached', 'DataCloneError');
                const state = stateForCanvas(value);
                if (state.mode === 'webgl') throw new DOMException('A Canvas with an active WebGL context cannot be transferred', 'InvalidStateError');
                if (!state.pixels) throw new DOMException('Canvas exceeds the bitmap budget', 'DataCloneError');
                return { width: state.width, height: state.height, pixels: state.pixels,
                    canvasWidth: value.width, canvasHeight: value.height,
                    alpha: bitmapRendererStates.get(state.context)?.alpha,
                    mode: state.mode, kind: 'offscreencanvas' };
            }
            const state = imageBitmapPixels(value);
            return { width: state.width, height: state.height, pixels: state.pixels,
                premultiplied: state.premultiplied, kind: 'imagebitmap' };
        },
        detach(value) {
            if (value instanceof OffscreenCanvas) {
                value.__detached = true;
                const state = canvasStates.get(value);
                if (state) { state.pixels = null; state.context = null; }
            } else closeImageBitmap(value);
        },
        receive(record, bytes) {
            const width = Number(record.w), height = Number(record.h);
            if (!Number.isInteger(width) || !Number.isInteger(height) || width < 0 || height < 0 ||
                width * height > MAX_CANVAS_PIXELS || bytes.length !== width * height * 4)
                throw new DOMException('Invalid bitmap transfer', 'DataCloneError');
            const pixels = new Uint8ClampedArray(bytes);
            if (record.t === 'imagebitmap') return makeImageBitmap(width, height, pixels, record.a === true);
            if (record.t !== 'offscreencanvas')
                throw new DOMException('Unknown bitmap transfer', 'DataCloneError');
            const canvasWidth = record.cw ?? width, canvasHeight = record.ch ?? height;
            if (!Number.isInteger(canvasWidth) || !Number.isInteger(canvasHeight) ||
                canvasWidth < 0 || canvasHeight < 0 || canvasWidth > 0xffffffff || canvasHeight > 0xffffffff)
                throw new DOMException('Invalid Canvas dimensions in transfer', 'DataCloneError');
            const canvas = new OffscreenCanvas(canvasWidth, canvasHeight), state = stateForCanvas(canvas);
            state.width = width; state.height = height;
            state.pixels = pixels;
            if (record.m === '2d') canvas.getContext('2d');
            else if (record.m === 'bitmaprenderer') canvas.getContext('bitmaprenderer', {alpha:record.o !== false});
            return canvas;
        }
    };
