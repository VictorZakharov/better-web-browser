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
    class OffscreenCanvas extends EventTarget {
        constructor(width, height) {
            super();
            if (arguments.length < 2) throw new TypeError('OffscreenCanvas requires width and height');
            this.__width = canvasUnsignedDimension(width);
            this.__height = canvasUnsignedDimension(height);
            this.__detached = false;
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
        getContext(contextId) {
            if (this.__detached) throw new DOMException('OffscreenCanvas is detached', 'InvalidStateError');
            const mode = String(contextId).toLowerCase();
            if (mode !== '2d' && mode !== 'bitmaprenderer') return null;
            const state = stateForCanvas(this);
            if (state.mode !== 'none' && state.mode !== mode) return null;
            state.mode = mode;
            return state.context ||= mode === '2d' ?
                new OffscreenCanvasRenderingContext2D(offscreenContextToken, this) :
                new ImageBitmapRenderingContext(canvasBitmapContextToken, this);
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
                if (!state.pixels) throw new DOMException('Canvas exceeds the bitmap budget', 'DataCloneError');
                return { width: state.width, height: state.height, pixels: state.pixels,
                    mode: state.mode, kind: 'offscreencanvas' };
            }
            const state = imageBitmapPixels(value);
            return { width: state.width, height: state.height, pixels: state.pixels, kind: 'imagebitmap' };
        },
        detach(value) {
            if (value instanceof OffscreenCanvas) {
                value.__detached = true;
                const state = canvasStates.get(value);
                if (state) { state.pixels = null; state.context = null; }
            } else value.close();
        },
        receive(record, bytes) {
            const width = Number(record.w), height = Number(record.h);
            if (!Number.isInteger(width) || !Number.isInteger(height) || width < 0 || height < 0 ||
                width * height > MAX_CANVAS_PIXELS || bytes.length !== width * height * 4)
                throw new DOMException('Invalid bitmap transfer', 'DataCloneError');
            const pixels = new Uint8ClampedArray(bytes);
            if (record.t === 'imagebitmap') return makeImageBitmap(width, height, pixels);
            if (record.t !== 'offscreencanvas')
                throw new DOMException('Unknown bitmap transfer', 'DataCloneError');
            const canvas = new OffscreenCanvas(width, height), state = stateForCanvas(canvas);
            state.pixels = pixels;
            if (record.m === '2d') canvas.getContext('2d');
            else if (record.m === 'bitmaprenderer') canvas.getContext('bitmaprenderer');
            return canvas;
        }
    };
