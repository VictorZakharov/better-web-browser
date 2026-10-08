    // This first Canvas 2D slice owns a bounded software bitmap. It intentionally exposes only
    // operations backed by real pixels; unsupported context types and APIs continue to fail closed.
    const MAX_CANVAS_PIXELS = 4 * 1024 * 1024;
    const canvasStates = new WeakMap();
    const canvas2dOwners = new WeakMap();
    const canvas2dContextToken = Symbol('CanvasRenderingContext2D');
    const canvasDrawingStates = new WeakMap();
    const canvasDrawingWeakGet = Function.call.bind(WeakMap.prototype.get);
    const canvasDrawingWeakSet = Function.call.bind(WeakMap.prototype.set);
    const canvasDrawingCreate = Object.create;
    const defineCanvasContextProperties = (prototype, descriptors) => {
        for (const descriptor of Object.values(descriptors)) {
            descriptor.configurable = true;
            descriptor.enumerable = true;
        }
        Object.defineProperties(prototype, descriptors);
    };
    const canvasDrawingState = context => {
        const state = canvasDrawingWeakGet(canvasDrawingStates, context);
        if (!state) throw new TypeError('Illegal CanvasRenderingContext2D receiver');
        return state;
    };
    let synchronizeWebGlCanvas = () => {};
    let resetWebGlCanvas = () => {};
    let dirtyWebGlCanvas = () => {};

    const canvasDimension = (element, name, fallback) => {
        const raw = canvasOwnerAttribute(element, name);
        const digits = raw === null ? null : /^[\t\n\f\r ]*\+?(\d+)/.exec(raw);
        if (!digits) return fallback;
        const value = Number(digits[1]);
        return value <= 2147483647 ? value : fallback;
    };

    const normalizedColor = value => {
        const result = host('normalizeCssColor', `${value}`);
        if (!result) return null;
        const [serialized, red, green, blue, alpha] = result.split('\u001f');
        return { serialized, channels: [Number(red), Number(green), Number(blue), Number(alpha)] };
    };

    const stateForCanvas = (canvas, forceReset = false) => {
        const [width, height] = canvasOwnedDimensions(canvas);
        let state = canvasPrivateWeakGet(canvasStates, canvas);
        if (!state) {
            state = { width: -1, height: -1, inputWidth: -1, inputHeight: -1,
                pixels: null, context: null, mode: 'none', placeholder: null };
            canvasPrivateWeakSet(canvasStates, canvas, state);
        }
        // bitmaprenderer owns its transferred bitmap's natural dimensions,
        // independently of the unchanged Canvas width/height content attributes.
        if (forceReset || state.inputWidth !== width || state.inputHeight !== height) {
            state.inputWidth = width;
            state.inputHeight = height;
            state.originClean = true;
            state.width = width;
            state.height = height;
            state.pixels = state.mode !== 'webgl' && width * height <= MAX_CANVAS_PIXELS
                ? new canvasPrivatePixelArray(width * height * 4)
                : null;
            if (state.mode === 'bitmaprenderer') resetCanvasBitmapRenderer(state.context);
            else if (state.mode === 'webgl') resetWebGlCanvas(state);
            else if (state.context) resetCanvasDrawingState(state.context);
            if (state.mode === '2d') canvasInitializeOutputBitmap(state.context, state.pixels);
        }
        if (state.mode === 'webgl') synchronizeWebGlCanvas(state);
        return state;
    };

    const normalizedRectangle = (x, y, width, height) => {
        x = +x;
        y = +y;
        width = +width;
        height = +height;
        if (![x, y, width, height].every(Number.isFinite)) return null;
        if (width < 0) { x += width; width = -width; }
        if (height < 0) { y += height; height = -height; }
        return { x, y, width, height };
    };

    class CanvasRenderingContext2D {
        constructor(canvas, token, settings) {
            if (token !== canvas2dContextToken) throw new TypeError('Illegal constructor');
            Object.defineProperty(this, 'canvas', { enumerable: true, value: canvas });
            canvasDrawingWeakSet(canvasDrawingStates, this, canvasDrawingCreate(null));
            canvasPrivateWeakSet(canvas2dOwners, this, canvas);
            resetCanvasDrawingState(this);
            canvasInitializeContextSettings(this, settings);
        }
        __reset() {
            canvasDrawingState(this).fill = normalizedColor('#000000');
            canvasDrawingState(this).globalAlpha = 1;
            canvasDrawingState(this).compositeOperation = 'source-over';
            canvasDrawingState(this).stroke = normalizedColor('#000000');
            canvasDrawingState(this).lineWidth = 1;
            canvasDrawingState(this).lineCap = 'butt';
            canvasDrawingState(this).lineJoin = 'miter';
            canvasDrawingState(this).miterLimit = 10;
            canvasDrawingState(this).lineDash = [];
            canvasDrawingState(this).dashOffset = 0;
            canvasDrawingState(this).imageSmoothingEnabled = true;
            canvasDrawingState(this).imageSmoothingQuality = 'low';
            canvasDrawingState(this).transform = identity2D();
            canvasDrawingState(this).clipBits = null;
            canvasDrawingState(this).shadowColor = normalizedColor('rgba(0, 0, 0, 0)');
            canvasDrawingState(this).shadowBlur = 0;
            canvasDrawingState(this).shadowOffsetX = 0;
            canvasDrawingState(this).shadowOffsetY = 0;
            canvasDrawingState(this).filter = 'none';
            canvasDrawingState(this).filterOperations = [];
            canvasDrawingState(this).font = '10px sans-serif';
            canvasDrawingState(this).fontSpec = host('canvasParseFont', canvasDrawingState(this).font);
            canvasDrawingState(this).textAlign = 'start';
            canvasDrawingState(this).textBaseline = 'alphabetic';
            canvasDrawingState(this).direction = 'inherit';
            canvasDrawingState(this).fontKerning = 'auto';
            canvasDrawingState(this).lang = 'inherit';
            canvasDrawingState(this).letterSpacing = ['0px', [0,0,0,0,0,0,0]];
            canvasDrawingState(this).wordSpacing = ['0px', [0,0,0,0,0,0,0]];
            canvasDrawingState(this).path = newCanvasPath();
            canvasDrawingState(this).stack = [];
        }
        get fillStyle() { return canvasIsGradient(canvasDrawingState(this).fill) ||
            canvasIsPattern(canvasDrawingState(this).fill) ? canvasDrawingState(this).fill : canvasDrawingState(this).fill.serialized; }
        set fillStyle(value) {
            if (canvasIsPattern(value) && canvasPatternGet(canvasPatternStates, value).source.originClean === false)
                stateForCanvas(this.canvas).originClean = false;
            if (canvasIsGradient(value) || canvasIsPattern(value)) {
                canvasDrawingState(this).fill = value; return;
            }
            const color = normalizedColor(value);
            if (color) canvasDrawingState(this).fill = color;
        }
        createLinearGradient(x0, y0, x1, y1) {
            canvasImageDataContext(this);
            if (arguments.length < 4) throw new TypeError('createLinearGradient requires four arguments');
            return canvasGradient('linear', [x0, y0, x1, y1]);
        }
        createRadialGradient(x0, y0, r0, x1, y1, r1) {
            canvasImageDataContext(this);
            if (arguments.length < 6) throw new TypeError('createRadialGradient requires six arguments');
            return canvasGradient('radial', [x0, y0, r0, x1, y1, r1]);
        }
        createConicGradient(startAngle, x, y) {
            canvasImageDataContext(this);
            if (arguments.length < 3) throw new TypeError('createConicGradient requires three arguments');
            return canvasGradient('conic', [startAngle, x, y]);
        }
        get globalAlpha() { return canvasDrawingState(this).globalAlpha; }
        set globalAlpha(value) {
            value = +value;
            if (Number.isFinite(value) && value >= 0 && value <= 1) canvasDrawingState(this).globalAlpha = value;
        }
        get globalCompositeOperation() { return canvasDrawingState(this).compositeOperation; }
        set globalCompositeOperation(value) {
            value = `${value}`;
            if (canvasCompositeOperators.has(value)) canvasDrawingState(this).compositeOperation = value;
        }
        save() {
            if (canvasDrawingState(this).stack.length < 64)
                canvasPrivatePush(canvasDrawingState(this).stack, { fill: canvasDrawingState(this).fill, globalAlpha: canvasDrawingState(this).globalAlpha,
                    compositeOperation: canvasDrawingState(this).compositeOperation, stroke: canvasDrawingState(this).stroke,
                    lineWidth: canvasDrawingState(this).lineWidth, lineCap: canvasDrawingState(this).lineCap,
                    lineJoin: canvasDrawingState(this).lineJoin, miterLimit: canvasDrawingState(this).miterLimit,
                    lineDash: [...canvasDrawingState(this).lineDash], dashOffset: canvasDrawingState(this).dashOffset,
                    imageSmoothingEnabled: canvasDrawingState(this).imageSmoothingEnabled,
                    imageSmoothingQuality: canvasDrawingState(this).imageSmoothingQuality,
                    transform: [...canvasDrawingState(this).transform], clipBits: canvasDrawingState(this).clipBits,
                    shadowColor: canvasDrawingState(this).shadowColor, shadowBlur: canvasDrawingState(this).shadowBlur,
                    shadowOffsetX: canvasDrawingState(this).shadowOffsetX, shadowOffsetY: canvasDrawingState(this).shadowOffsetY,
                    filter: canvasDrawingState(this).filter, filterOperations: canvasDrawingState(this).filterOperations,
                    text: { font: canvasDrawingState(this).font, fontSpec: canvasDrawingState(this).fontSpec,
                        textAlign: canvasDrawingState(this).textAlign, textBaseline: canvasDrawingState(this).textBaseline,
                        direction: canvasDrawingState(this).direction,
                        fontKerning: canvasDrawingState(this).fontKerning,
                        lang: canvasDrawingState(this).lang,
                        letterSpacing: canvasDrawingState(this).letterSpacing,
                        wordSpacing: canvasDrawingState(this).wordSpacing } });
        }
        restore() {
            const state = canvasPrivatePop(canvasDrawingState(this).stack);
            if (state) {
                canvasDrawingState(this).fill = state.fill;
                canvasDrawingState(this).globalAlpha = state.globalAlpha;
                canvasDrawingState(this).compositeOperation = state.compositeOperation;
                canvasDrawingState(this).stroke = state.stroke;
                canvasDrawingState(this).lineWidth = state.lineWidth;
                canvasDrawingState(this).lineCap = state.lineCap;
                canvasDrawingState(this).lineJoin = state.lineJoin;
                canvasDrawingState(this).miterLimit = state.miterLimit;
                canvasDrawingState(this).lineDash = state.lineDash;
                canvasDrawingState(this).dashOffset = state.dashOffset;
                canvasDrawingState(this).imageSmoothingEnabled = state.imageSmoothingEnabled;
                canvasDrawingState(this).imageSmoothingQuality = state.imageSmoothingQuality;
                canvasDrawingState(this).transform = state.transform;
                canvasDrawingState(this).clipBits = state.clipBits;
                canvasDrawingState(this).shadowColor = state.shadowColor;
                canvasDrawingState(this).shadowBlur = state.shadowBlur;
                canvasDrawingState(this).shadowOffsetX = state.shadowOffsetX;
                canvasDrawingState(this).shadowOffsetY = state.shadowOffsetY;
                canvasDrawingState(this).filter = state.filter;
                canvasDrawingState(this).filterOperations = state.filterOperations;
                canvasDrawingState(this).font = state.text.font;
                canvasDrawingState(this).fontSpec = state.text.fontSpec;
                canvasDrawingState(this).textAlign = state.text.textAlign;
                canvasDrawingState(this).textBaseline = state.text.textBaseline;
                canvasDrawingState(this).direction = state.text.direction;
                canvasDrawingState(this).fontKerning = state.text.fontKerning;
                canvasDrawingState(this).lang = state.text.lang;
                canvasDrawingState(this).letterSpacing = state.text.letterSpacing;
                canvasDrawingState(this).wordSpacing = state.text.wordSpacing;
            }
        }
        clearRect(x, y, width, height) { paintCanvasRectangle(this, x, y, width, height, null); }
        fillRect(x, y, width, height) { paintCanvasRectangle(this, x, y, width, height, canvasDrawingState(this).fill); }
        __paintRect(x, y, width, height, style) {
            const rect = normalizedRectangle(x, y, width, height);
            const state = stateForCanvas(this.canvas);
            if (!rect || !rect.width || !rect.height || !state.pixels) return;
            paintTransformedCanvasRect(this, state, rect, style);
        }
        createImageData(widthOrImageData, height, settings) {
            return createCanvasImageData(this, arguments);
        }
        getImageData(x, y, width, height, settings) {
            if (arguments.length < 4) throw new TypeError('getImageData requires four coordinates');
            return readCanvasImageData(this, x, y, width, height, settings);
        }
        putImageData(imageData, x, y, ...dirty) {
            if (arguments.length < 3) throw new TypeError('putImageData requires ImageData and coordinates');
            writeCanvasImageData(this, imageData, x, y, dirty);
        }
        getContextAttributes() { return canvasGetContextSettings(this); }
        reset() {
            const state = stateForCanvas(this.canvas);
            if (state.pixels) canvasClearBitmapRange(state.pixels, 0, canvasPrivateCount(state.pixels));
            resetCanvasDrawingState(this);
        }
        isContextLost() { return false; }
    }
    // Implementation helpers are captured, not author-visible prototype hooks.
    // Drawing state is platform-owned and never consulted through context expandos.
    const canvasResetImplementation = CanvasRenderingContext2D.prototype.__reset;
    const canvasRectangleImplementation = CanvasRenderingContext2D.prototype.__paintRect;
    delete CanvasRenderingContext2D.prototype.__reset;
    delete CanvasRenderingContext2D.prototype.__paintRect;
    const resetCanvasDrawingState = context => canvasPathApply(canvasResetImplementation, context, []);
    const paintCanvasRectangle = (context, x, y, width, height, style) =>
        canvasPathApply(canvasRectangleImplementation, context, [x, y, width, height, style]);

    class HTMLCanvasElement extends HTMLElement {
        get width() { return canvasDimension(this, 'width', 300); }
        set width(value) {
            setCanvasDimension(this, 'width', value, 300);
        }
        get height() { return canvasDimension(this, 'height', 150); }
        set height(value) {
            setCanvasDimension(this, 'height', value, 150);
        }
        getContext(contextId, options = undefined) {
            canvasHtmlReceiver(this);
            if (!arguments.length) throw new TypeError('getContext requires a context identifier');
            const requested = `${contextId}`;
            const mode = ['experimental-webgl','webgl2'].includes(requested) ? 'webgl' : requested;
            if (!['2d', 'bitmaprenderer', 'webgl'].includes(mode)) return null;
            const state = stateForCanvas(this);
            if (state.mode !== 'none' && state.mode !== mode) return null;
            if (state.context) return mode === 'webgl' &&
                webGlState(state.context).api !== (requested === 'webgl2' ? 'webgl2' : 'webgl1') ? null : state.context;
            const settings = mode === '2d' ? canvasConvertSettings(options) : null;
            if (settings && !canvasSettingsSupported(settings)) return null;
            const context = mode === 'webgl' ? createWebGlContext(this, options, requested === 'webgl2' ? 'webgl2' : 'webgl1') : mode === '2d' ? new CanvasRenderingContext2D(this, canvas2dContextToken, settings) :
                new ImageBitmapRenderingContext(canvasBitmapContextToken, this, options);
            if (!context) return null;
            state.context = context;
            state.mode = mode;
            return context;
        }
    }
