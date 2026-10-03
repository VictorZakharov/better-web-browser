    // Private context/object brands keep driver IDs out of author-visible properties.
    // https://registry.khronos.org/webgl/specs/latest/1.0/
    const webGlToken = Symbol('WebGL native construction');
    const webGlContexts = new WeakMap();
    const webGlObjects = new WeakMap();
    const webGlObjectClasses = {};
    const webGlEventMessages = new WeakMap();
    class WebGLContextEvent extends Event {
        constructor(type, options = {}) {
            super(type, options);
            webGlEventMessages.set(this, String(options?.statusMessage ?? ''));
        }
        get statusMessage() {
            if (!webGlEventMessages.has(this)) throw new TypeError('Illegal WebGLContextEvent receiver');
            return webGlEventMessages.get(this);
        }
    }
    Object.defineProperty(WebGLContextEvent.prototype, Symbol.toStringTag, {value:'WebGLContextEvent'});
    Object.defineProperty(globalThis, 'WebGLContextEvent', {configurable:true, writable:true, value:WebGLContextEvent});
    for (const name of ['WebGLBuffer', 'WebGLShader', 'WebGLProgram', 'WebGLTexture',
        'WebGLFramebuffer', 'WebGLRenderbuffer', 'WebGLUniformLocation', 'WebGLActiveInfo',
        'WebGLShaderPrecisionFormat']) {
        const constructor = class {
            constructor(token) { if (token !== webGlToken) throw new TypeError('Illegal constructor'); }
        };
        Object.defineProperty(constructor, 'name', {value:name});
        Object.defineProperty(constructor.prototype, Symbol.toStringTag, {value:name});
        webGlObjectClasses[name] = constructor;
        Object.defineProperty(globalThis, name, {configurable:true, writable:true, value:constructor});
    }
    const webGlState = context => {
        const state = webGlContexts.get(context);
        if (!state) throw new TypeError('Illegal WebGL receiver');
        return state;
    };
    const webGlError = (context, error) => {
        const state = webGlState(context);
        if (state.lost) {
            if (error === 0x0502) state.lossError = error;
        } else host('webglCommand', state.id, JSON.stringify({op:'bridgeError', i:[error]}));
    };
    const webGlCall = (context, op, i = [], f = [], text = '', bytes = undefined) => {
        const state = webGlState(context);
        if (state.lost) return null;
        if (!i.every(Number.isSafeInteger) || !f.every(value => typeof value === 'number')) {
            webGlError(context, 0x0501); return null;
        }
        const encoded = f.map(value => Object.is(value, -0) ? '-0' : Number.isFinite(value) ? value : Number.isNaN(value) ? 'nan' : value > 0 ? 'inf' : '-inf');
        const raw = host(op==='readPixels'?'webglReadPixels':'webglCommand', state.id, JSON.stringify({op, i, f:encoded, text}), bytes);
        if (raw instanceof Uint8Array) return raw;
        // Ordinary scalar/array replies do not need a recursive reviver walk.
        // Only native non-JSON float sentinels require the special conversion.
        const value = raw ? JSON.parse(raw, raw.includes('"webglFloat"') ? (key, entry) => {
            if (entry && typeof entry === 'object' && Object.keys(entry).length === 1 && 'webglFloat' in entry)
                return entry.webglFloat === '-0' ? -0 : entry.webglFloat === 'nan' ? NaN : entry.webglFloat === 'inf' ? Infinity : -Infinity;
            return entry;
        } : undefined) : null;
        if (value?.lost) {
            loseWebGlContext(context, false);
            return null;
        }
        return value;
    };
    const webGlHandle = (context, value, type, nullable = false) => {
        if (value === null && nullable) return 0;
        const state = webGlState(context), object = webGlObjects.get(value);
        if (!object || object.type !== type) throw new TypeError('Expected ' + type);
        if (object.context !== context || object.epoch !== state.epoch || state.lost) {
            webGlError(context, 0x0502); return -1;
        }
        return object.id;
    };
    const webGlObject = (context, type, id) => {
        if (!id) return null;
        const state = webGlState(context);
        const previous = state.objects.get(id);
        if (previous) return previous;
        const object = new webGlObjectClasses[type](webGlToken);
        webGlObjects.set(object, {context, type, id, epoch:state.epoch, deleted:false});
        state.objects.set(id, object);
        return object;
    };
    const webGlBytes = value => {
        if (value instanceof ArrayBuffer) return new Uint8Array(value);
        if (ArrayBuffer.isView(value)) return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
        throw new TypeError('Expected an ArrayBuffer or ArrayBufferView');
    };
    const webGlInteger = value => Number(value) | 0;
    const webGlUnsigned = value => Number(value) >>> 0;
    class WebGLRenderingContext {
        constructor(token) { if (token !== webGlToken) throw new TypeError('Illegal constructor'); }
        get canvas() { return webGlState(this).canvas; }
        get drawingBufferWidth() { const s = webGlState(this); stateForCanvas(s.canvas); return s.lost ? 0 : Math.max(1, s.canvas.width); }
        get drawingBufferHeight() { const s = webGlState(this); stateForCanvas(s.canvas); return s.lost ? 0 : Math.max(1, s.canvas.height); }
        getContextAttributes() { const s = webGlState(this); return s.lost ? null : {...s.attributes}; }
        isContextLost() { return webGlState(this).lost; }
        getSupportedExtensions() {
            const state = webGlState(this);
            if (state.lost) return null;
            return ['WEBGL_lose_context', ...(webGlCall(this, 'supportedExtensions') ?? [])];
        }
        getExtension(name) {
            webGlState(this);
            if (arguments.length < 1) throw new TypeError('getExtension requires 1 argument');
            return webGlExtension(this, name);
        }
        getError() {
            const s = webGlState(this);
            if (s.lost) {
                if (!s.lossReported) { s.lossReported = true; return 0x9242; }
                const error = s.lossError ?? 0; s.lossError = 0; return error;
            }
            return webGlCall(this, 'getError') ?? 0;
        }
    }
    Object.defineProperty(WebGLRenderingContext.prototype, Symbol.toStringTag, {value:'WebGLRenderingContext'});
    Object.defineProperty(globalThis, 'WebGLRenderingContext', {configurable:true, writable:true, value:WebGLRenderingContext});
    const createWebGlContext = (canvas, requested = {}) => {
        const attributes = webGlContextAttributes(requested);
        // WARP is a real software GLES driver, not a promise of hardware acceleration.
        const creationFailed = message => {
            canvas.dispatchEvent(markTrusted(new WebGLContextEvent('webglcontextcreationerror',
                {statusMessage:message, cancelable:true})));
            return null;
        };
        if (attributes.failIfMajorPerformanceCaveat)
            return creationFailed('The available ANGLE/WARP backend is software rendered');
        const id = host('webglCreate', Math.max(1, canvas.width), Math.max(1, canvas.height),
            JSON.stringify({alpha:attributes.alpha, depth:attributes.depth,
                stencil:attributes.stencil, preserve:attributes.preserveDrawingBuffer}));
        if (!id) return creationFailed('ANGLE/WARP is unavailable or the WebGL context/drawing-buffer budget was exceeded');
        const context = new WebGLRenderingContext(webGlToken);
        webGlContexts.set(context, {id, canvas, attributes, objects:new Map(), lost:false, epoch:0,
            extensions:new Map(), lossReported:false, lossError:0,
            dirty:true, unpackFlip:false, unpackPremultiply:false, unpackColorSpace:0x9244});
        return context;
    };
    synchronizeWebGlCanvas = state => {
        const native = webGlContexts.get(state.context);
        if (!native || native.lost || !native.dirty) return;
        const snapshot = host('webglSnapshot', native.id);
        if (!snapshot || snapshot[2].length !== snapshot[0] * snapshot[1] * 4) return;
        const pixels = new Uint8ClampedArray(snapshot[2]);
        if (native.attributes.alpha && native.attributes.premultipliedAlpha) {
            for (let offset = 0; offset < pixels.length; offset += 4) {
                const alpha = pixels[offset + 3];
                for (let channel = 0; channel < 3; channel++)
                    pixels[offset + channel] = alpha ? Math.min(255, Math.round(pixels[offset + channel] * 255 / alpha)) : 0;
            }
        }
        state.pixels = pixels;
        native.dirty = false;
    };
    resetWebGlCanvas = state => {
        const native = webGlContexts.get(state.context);
        if (!native || native.lost) return;
        webGlCall(state.context, 'resize', [native.canvas.width, native.canvas.height]);
        native.dirty = true;
    };
    const webGlPresented = state => {
        const native = webGlContexts.get(state.context);
        if (!native || native.lost || native.attributes.preserveDrawingBuffer) return;
        webGlCall(state.context, 'presented');
        native.dirty = true;
    };
