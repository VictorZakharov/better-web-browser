    // Private context/object brands keep driver IDs out of author-visible properties.
    // https://registry.khronos.org/webgl/specs/latest/1.0/
    const webGlToken = Symbol('WebGL native construction');
    const webGlContexts = webGlPrivateBrands();
    const webGlObjects = webGlPrivateBrands();
    const webGlObjectClasses = {};
    const webGlEventMessages = webGlPrivateBrands();
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
    Object.defineProperty(WebGLContextEvent.prototype, 'statusMessage', {enumerable:true});
    Object.defineProperty(globalThis, 'WebGLContextEvent', {configurable:true, writable:true, value:WebGLContextEvent});
    class WebGLObject {
        constructor(token) { if (token !== webGlToken) throw new TypeError('Illegal constructor'); }
    }
    Object.defineProperty(WebGLObject.prototype, Symbol.toStringTag, {value:'WebGLObject'});
    Object.defineProperty(WebGLObject, 'length', {value:0});
    Object.defineProperty(globalThis, 'WebGLObject', {configurable:true, writable:true, value:WebGLObject});
    for (const name of ['WebGLBuffer', 'WebGLShader', 'WebGLProgram', 'WebGLTexture',
        'WebGLFramebuffer', 'WebGLRenderbuffer', 'WebGLUniformLocation', 'WebGLActiveInfo',
        'WebGLShaderPrecisionFormat']) {
        const constructor = class {
            constructor(token) { if (token !== webGlToken) throw new TypeError('Illegal constructor'); }
        };
        Object.defineProperty(constructor, 'name', {value:name});
        Object.defineProperty(constructor, 'length', {value:0});
        // Resource interfaces inherit WebGLObject; reflection records and
        // uniform locations are separate IDL interfaces, not GPU resources.
        if (!['WebGLUniformLocation','WebGLActiveInfo','WebGLShaderPrecisionFormat'].includes(name)) {
            Object.setPrototypeOf(constructor.prototype,WebGLObject.prototype);
            Object.setPrototypeOf(constructor,WebGLObject);
        }
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
        } else {
            flushWebGlCommands();
            if (!state.lost) host('webglCommand', state.id, webGlWireCommand('bridgeError', [error]));
        }
    };
    const webGlCall = (context, op, i = [], f = [], text = '', bytes = undefined) => {
        const state = webGlState(context);
        if (state.lost) return null;
        if (queueWebGlNumericPacket(context,state,op,i,f,text,bytes)) return null;
        if (webGlWireNumericCandidate(op, i, f, text, bytes)) {
            flushWebGlCommands();
            if (state.lost) return null;
            // Lists have completed IDL conversion. A small fixed Float32 view
            // may retain its source only when no later author conversion can
            // run. Native admission copies synchronously before queueing; a
            // second JS copy/prototype transition adds no ownership guarantee.
            const live = host('webglCommandValues', state.id, op, i, f);
            if (!live) loseWebGlContext(context, false);
            return null;
        }
        flushWebGlCommands();
        if (state.lost) return null;
        const command = webGlWireCommand(op, i, f, text);
        if (command === null) {
            webGlError(context, 0x0501); return null;
        }
        const raw = host(op==='readPixels'||op==='getBufferSubData'?'webglReadPixels':'webglCommand', state.id, command, bytes);
        if (webGlWireBytes(raw)) return raw;
        // Ordinary scalar/array replies do not need a recursive reviver walk.
        // Only native non-JSON float sentinels require the special conversion.
        const value = raw ? webGlWireParse(raw) : null;
        if (webGlWireLost(value)) {
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
        if (value === null) throw new TypeError('Expected an ArrayBuffer or ArrayBufferView');
        return webGl2Source(value).bytes;
    };
    const webGlInteger = value => Number(value) | 0;
    const webGlUnsigned = value => Number(value) >>> 0;
    class WebGLRenderingContext {
        constructor(token) { if (token !== webGlToken) throw new TypeError('Illegal constructor'); }
        get canvas() { return webGlState(this).canvas; }
        get drawingBufferWidth() { const s = webGlState(this); stateForCanvas(s.canvas); return s.lost ? 0 : (webGlCall(this, 'drawingBufferSize')?.[0] ?? 0); }
        get drawingBufferHeight() { const s = webGlState(this); stateForCanvas(s.canvas); return s.lost ? 0 : (webGlCall(this, 'drawingBufferSize')?.[1] ?? 0); }
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
    Object.defineProperty(WebGLRenderingContext, 'length', {value:0});
    // JavaScript class syntax defaults to non-enumerable members; Web IDL
    // operations and regular attributes are enumerable on the prototype.
    for (const name of Object.getOwnPropertyNames(WebGLRenderingContext.prototype)) {
        if (name !== 'constructor') Object.defineProperty(WebGLRenderingContext.prototype, name, {enumerable:true});
    }
    Object.defineProperty(globalThis, 'WebGLRenderingContext', {configurable:true, writable:true, value:WebGLRenderingContext});
    const createWebGlContext = (canvas, requested = {}, api = 'webgl1') => {
        const attributes = webGlContextAttributes(requested, api);
        flushWebGlCommands();
        const creationFailed = message => {
            canvas.dispatchEvent(markTrusted(new WebGLContextEvent('webglcontextcreationerror',
                {statusMessage:message, cancelable:true})));
            return null;
        };
        const id = host('webglCreate', Math.max(1, canvas.width), Math.max(1, canvas.height),
            webGlNativeOptions(attributes, api));
        if (!id) return creationFailed(attributes.failIfMajorPerformanceCaveat
            ? 'No suitable hardware ANGLE context is available within the drawing-buffer budget; software fallback is disabled by failIfMajorPerformanceCaveat'
            : 'No ANGLE backend satisfies the requested WebGL attributes and context/drawing-buffer budget');
        const context = api === 'webgl2' ? new WebGL2RenderingContext(webGlToken) : new WebGLRenderingContext(webGlToken);
        webGlContexts.set(context, {id, api, canvas, attributes, objects:new Map(), lost:false, epoch:0,
            extensions:new Map(), lossReported:false, lossError:0,
            dirty:true, unpackFlip:false, unpackPremultiply:false, unpackColorSpace:0x9244});
        return context;
    };
    synchronizeWebGlCanvas = state => {
        const native = webGlContexts.get(state.context);
        if (!native || native.lost || !native.dirty) return;
        flushWebGlCommands();
        if (native.lost) return;
        const snapshot = host('webglSnapshot', native.id);
        if (!snapshot || snapshot[2].length !== snapshot[0] * snapshot[1] * 4) return;
        const pixels = new Uint8ClampedArray(snapshot[2]);
        // The native owner returns straight RGBA for bitmap consumers. Raw GL
        // readPixels still sees the original drawing buffer representation.
        // Native allocation can be smaller than the unchanged canvas content
        // attributes. Every bitmap consumer must use the snapshot's extent.
        state.width = snapshot[0];
        state.height = snapshot[1];
        state.pixels = pixels;
        native.dirty = false;
    };
    resetWebGlCanvas = state => {
        const native = webGlContexts.get(state.context);
        if (!native || native.lost) return;
        webGlCall(state.context, 'resizeCanvas', [native.canvas.width, native.canvas.height]);
        native.dirty = true;
    };
    const webGlPresented = state => {
        const native = webGlContexts.get(state.context);
        if (!native || native.lost) return;
        webGlCall(state.context, 'presented');
        if (!native.attributes.preserveDrawingBuffer) native.dirty = true;
    };
