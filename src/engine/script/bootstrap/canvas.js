    // This first Canvas 2D slice owns a bounded software bitmap. It intentionally exposes only
    // operations backed by real pixels; unsupported context types and APIs continue to fail closed.
    const MAX_CANVAS_PIXELS = 4 * 1024 * 1024;
    const canvasStates = new WeakMap();
    const canvas2dOwners = new WeakMap();
    let synchronizeWebGlCanvas = () => {};
    let resetWebGlCanvas = () => {};
    let dirtyWebGlCanvas = () => {};

    const canvasDimension = (element, name, fallback) => {
        const raw = element.getAttribute(name);
        if (raw === null || raw.trim() === '' || !/^\d+$/.test(raw.trim())) return fallback;
        return Math.min(0xffffffff, Number(raw));
    };

    const imageDataStates = new WeakMap();
    class ImageData {
        constructor(dataOrWidth, widthOrHeight, heightOrSettings, settings = {}) {
            let data;
            let width;
            let height;
            if (dataOrWidth instanceof Uint8ClampedArray) {
                data = dataOrWidth;
                width = Math.trunc(Number(widthOrHeight));
                height = heightOrSettings === undefined || typeof heightOrSettings === 'object'
                    ? data.length / 4 / width
                    : Math.trunc(Number(heightOrSettings));
                settings = (typeof heightOrSettings === 'object' ? heightOrSettings : settings) || {};
                if (width <= 0 || height <= 0 || !Number.isInteger(height) ||
                    data.length !== width * height * 4)
                    throw new DOMException('ImageData dimensions do not match its data', 'IndexSizeError');
                if (width * height > MAX_CANVAS_PIXELS)
                    throw new DOMException('ImageData exceeds the bitmap budget', 'NotSupportedError');
            } else {
                width = Math.trunc(Number(dataOrWidth));
                height = Math.trunc(Number(widthOrHeight));
                settings = heightOrSettings || {};
                if (width <= 0 || height <= 0)
                    throw new DOMException('ImageData dimensions must be positive', 'IndexSizeError');
                if (width * height > MAX_CANVAS_PIXELS)
                    throw new DOMException('ImageData exceeds the bitmap budget', 'NotSupportedError');
                data = new Uint8ClampedArray(width * height * 4);
            }
            if (settings.colorSpace !== undefined && settings.colorSpace !== 'srgb')
                throw new TypeError('Only the srgb ImageData color space is supported');
            Object.defineProperties(this, {
                data: { enumerable: true, value: data },
                width: { enumerable: true, value: width },
                height: { enumerable: true, value: height },
                colorSpace: { enumerable: true, value: 'srgb' }
            });
            imageDataStates.set(this,{data,width,height});
        }
    }

    const normalizedColor = value => {
        const result = host('normalizeCssColor', String(value));
        if (!result) return null;
        const [serialized, red, green, blue, alpha] = result.split('\u001f');
        return { serialized, channels: [Number(red), Number(green), Number(blue), Number(alpha)] };
    };

    const stateForCanvas = (canvas, forceReset = false) => {
        const width = canvas.width;
        const height = canvas.height;
        let state = canvasStates.get(canvas);
        if (!state) {
            state = { width: -1, height: -1, inputWidth: -1, inputHeight: -1,
                pixels: null, context: null, mode: 'none', placeholder: null };
            canvasStates.set(canvas, state);
        }
        // bitmaprenderer owns its transferred bitmap's natural dimensions,
        // independently of the unchanged Canvas width/height content attributes.
        if (forceReset || state.inputWidth !== width || state.inputHeight !== height) {
            state.inputWidth = width;
            state.inputHeight = height;
            state.width = width;
            state.height = height;
            state.pixels = state.mode !== 'webgl' && width * height <= MAX_CANVAS_PIXELS
                ? new Uint8ClampedArray(width * height * 4)
                : null;
            if (state.mode === 'bitmaprenderer') resetCanvasBitmapRenderer(state.context);
            else if (state.mode === 'webgl') resetWebGlCanvas(state);
            else state.context?.__reset?.();
        }
        if (state.mode === 'webgl') synchronizeWebGlCanvas(state);
        return state;
    };

    const normalizedRectangle = (x, y, width, height) => {
        x = Math.trunc(Number(x));
        y = Math.trunc(Number(y));
        width = Math.trunc(Number(width));
        height = Math.trunc(Number(height));
        if (![x, y, width, height].every(Number.isFinite)) return null;
        if (width < 0) { x += width; width = -width; }
        if (height < 0) { y += height; height = -height; }
        return { x, y, width, height };
    };

    class CanvasRenderingContext2D {
        constructor(canvas) {
            Object.defineProperty(this, 'canvas', { enumerable: true, value: canvas });
            canvas2dOwners.set(this, canvas);
            this.__reset();
        }
        __reset() {
            this.__fill = normalizedColor('#000000');
            this.__globalAlpha = 1;
            this.__compositeOperation = 'source-over';
            this.__stroke = normalizedColor('#000000');
            this.__lineWidth = 1;
            this.__lineCap = 'butt';
            this.__lineJoin = 'miter';
            this.__miterLimit = 10;
            this.__lineDash = [];
            this.__dashOffset = 0;
            this.__imageSmoothingEnabled = true;
            this.__imageSmoothingQuality = 'low';
            this.__transform = identity2D();
            this.__clipBits = null;
            this.__shadowColor = normalizedColor('rgba(0, 0, 0, 0)');
            this.__shadowBlur = 0;
            this.__shadowOffsetX = 0;
            this.__shadowOffsetY = 0;
            this.__filter = 'none';
            this.__filterOperations = [];
            this.__font = '10px sans-serif';
            this.__fontSpec = host('canvasParseFont', this.__font);
            this.__textAlign = 'start';
            this.__textBaseline = 'alphabetic';
            this.__direction = 'inherit';
            this.__path = newCanvasPath();
            this.__stack = [];
        }
        get fillStyle() { return this.__fill instanceof CanvasGradient ||
            this.__fill instanceof CanvasPattern ? this.__fill : this.__fill.serialized; }
        set fillStyle(value) {
            if (value instanceof CanvasGradient || value instanceof CanvasPattern) {
                this.__fill = value; return;
            }
            const color = normalizedColor(value);
            if (color) this.__fill = color;
        }
        createLinearGradient(x0, y0, x1, y1) { return canvasGradient('linear', [x0, y0, x1, y1]); }
        createRadialGradient(x0, y0, r0, x1, y1, r1) {
            return canvasGradient('radial', [x0, y0, r0, x1, y1, r1]);
        }
        createConicGradient(startAngle, x, y) {
            return canvasGradient('conic', [startAngle, x, y]);
        }
        get globalAlpha() { return this.__globalAlpha; }
        set globalAlpha(value) {
            value = Number(value);
            if (Number.isFinite(value) && value >= 0 && value <= 1) this.__globalAlpha = value;
        }
        get globalCompositeOperation() { return this.__compositeOperation; }
        set globalCompositeOperation(value) {
            value = String(value);
            if (canvasCompositeOperators.has(value)) this.__compositeOperation = value;
        }
        save() {
            if (this.__stack.length < 64)
                this.__stack.push({ fill: this.__fill, globalAlpha: this.__globalAlpha,
                    compositeOperation: this.__compositeOperation, stroke: this.__stroke,
                    lineWidth: this.__lineWidth, lineCap: this.__lineCap,
                    lineJoin: this.__lineJoin, miterLimit: this.__miterLimit,
                    lineDash: [...this.__lineDash], dashOffset: this.__dashOffset,
                    imageSmoothingEnabled: this.__imageSmoothingEnabled,
                    imageSmoothingQuality: this.__imageSmoothingQuality,
                    transform: [...this.__transform], clipBits: this.__clipBits,
                    shadowColor: this.__shadowColor, shadowBlur: this.__shadowBlur,
                    shadowOffsetX: this.__shadowOffsetX, shadowOffsetY: this.__shadowOffsetY,
                    filter: this.__filter, filterOperations: this.__filterOperations,
                    text: { font: this.__font, fontSpec: this.__fontSpec,
                        textAlign: this.__textAlign, textBaseline: this.__textBaseline,
                        direction: this.__direction } });
        }
        restore() {
            const state = this.__stack.pop();
            if (state) {
                this.__fill = state.fill;
                this.__globalAlpha = state.globalAlpha;
                this.__compositeOperation = state.compositeOperation;
                this.__stroke = state.stroke;
                this.__lineWidth = state.lineWidth;
                this.__lineCap = state.lineCap;
                this.__lineJoin = state.lineJoin;
                this.__miterLimit = state.miterLimit;
                this.__lineDash = state.lineDash;
                this.__dashOffset = state.dashOffset;
                this.__imageSmoothingEnabled = state.imageSmoothingEnabled;
                this.__imageSmoothingQuality = state.imageSmoothingQuality;
                this.__transform = state.transform;
                this.__clipBits = state.clipBits;
                this.__shadowColor = state.shadowColor;
                this.__shadowBlur = state.shadowBlur;
                this.__shadowOffsetX = state.shadowOffsetX;
                this.__shadowOffsetY = state.shadowOffsetY;
                this.__filter = state.filter;
                this.__filterOperations = state.filterOperations;
                this.__font = state.text.font;
                this.__fontSpec = state.text.fontSpec;
                this.__textAlign = state.text.textAlign;
                this.__textBaseline = state.text.textBaseline;
                this.__direction = state.text.direction;
            }
        }
        clearRect(x, y, width, height) { this.__paintRect(x, y, width, height, null); }
        fillRect(x, y, width, height) { this.__paintRect(x, y, width, height, this.__fill); }
        __paintRect(x, y, width, height, style) {
            const rect = normalizedRectangle(x, y, width, height);
            const state = stateForCanvas(this.canvas);
            if (!rect || !state.pixels) return;
            if (!canvasIsIdentity(this.__transform)) {
                paintTransformedCanvasRect(this, state, rect, style);
                return;
            }
            const left = Math.max(0, rect.x);
            const top = Math.max(0, rect.y);
            const right = Math.min(state.width, rect.x + rect.width);
            const bottom = Math.min(state.height, rect.y + rect.height);
            for (let row = top; row < bottom; row++) for (let column = left; column < right; column++) {
                if (!canvasClipAllows(this, column, row, state.width)) continue;
                const offset = (row * state.width + column) * 4;
                if (!style) {
                    state.pixels.fill(0, offset, offset + 4);
                    continue;
                }
                compositeCanvasPixel(state.pixels, offset, canvasPaintAt(style, column + 0.5, row + 0.5),
                    this.__globalAlpha, this.__compositeOperation);
            }
        }
        createImageData(widthOrImageData, height, settings) {
            if (widthOrImageData instanceof ImageData)
                return new ImageData(widthOrImageData.width, widthOrImageData.height, height);
            return new ImageData(Math.abs(Number(widthOrImageData)), Math.abs(Number(height)), settings);
        }
        getImageData(x, y, width, height, settings) {
            return readCanvasImageData(this, x, y, width, height, settings);
        }
        putImageData(imageData, x, y, ...dirty) {
            writeCanvasImageData(this, imageData, x, y, dirty);
        }
        getContextAttributes() { return { alpha: true, colorSpace: 'srgb', desynchronized: false, willReadFrequently: false }; }
        reset() {
            const state = stateForCanvas(this.canvas);
            state.pixels?.fill(0);
            this.__reset();
        }
        isContextLost() { return false; }
    }

    class HTMLCanvasElement extends HTMLElement {
        get width() { return canvasDimension(this, 'width', 300); }
        set width(value) {
            this.setAttribute('width', Math.max(0, Math.trunc(Number(value))) || 0);
            stateForCanvas(this, true);
        }
        get height() { return canvasDimension(this, 'height', 150); }
        set height(value) {
            this.setAttribute('height', Math.max(0, Math.trunc(Number(value))) || 0);
            stateForCanvas(this, true);
        }
        getContext(contextId, options = undefined) {
            const requested = String(contextId);
            const mode = ['experimental-webgl','webgl2'].includes(requested) ? 'webgl' : requested;
            if (!['2d', 'bitmaprenderer', 'webgl'].includes(mode)) return null;
            const state = stateForCanvas(this);
            if (state.mode !== 'none' && state.mode !== mode) return null;
            if (state.context) return mode === 'webgl' &&
                webGlState(state.context).api !== (requested === 'webgl2' ? 'webgl2' : 'webgl1') ? null : state.context;
            const context = mode === 'webgl' ? createWebGlContext(this, options, requested === 'webgl2' ? 'webgl2' : 'webgl1') : mode === '2d' ? new CanvasRenderingContext2D(this) :
                new ImageBitmapRenderingContext(canvasBitmapContextToken, this, options);
            if (!context) return null;
            state.context = context;
            state.mode = mode;
            return context;
        }
    }
