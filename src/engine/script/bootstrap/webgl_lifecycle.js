    // WebGL 1 context loss is a task, not a synchronous callback or microtask.
    // Keep the task scheduler private so author replacement of setTimeout cannot
    // turn native resource retirement into reentrant script execution.
    // https://registry.khronos.org/webgl/specs/latest/1.0/#5.15.2
    const webGlQueueTask = callback => queueWebGlContextTask(callback);
    const webGlLoseExtensions = new WeakMap();
    const loseWebGlContext = (context, simulated) => {
        const state = webGlState(context);
        if (state.lost) { webGlError(context, 0x0502); return; }
        const id = state.id;
        state.id = 0;
        state.lost = true;
        state.lossReported = false;
        state.lossError = 0;
        state.lossEventCompleted = false;
        state.restoreAllowed = false;
        state.restoreQueued = false;
        state.simulatedLoss = simulated;
        state.epoch++;
        state.objects.clear();
        state.extensions.clear();
        state.dirty = true;
        // Release the real driver context. Restoration creates a new one rather
        // than toggling the lost flag around surviving driver objects/state.
        host('webglDestroy', id);
        const epoch = state.epoch;
        webGlQueueTask(() => {
            if (!state.lost || state.epoch !== epoch) return;
            const event = markTrusted(new WebGLContextEvent('webglcontextlost', {cancelable:true}));
            state.canvas.dispatchEvent(event);
            state.restoreAllowed = event.defaultPrevented;
            state.lossEventCompleted = true;
            if (!simulated && state.restoreAllowed) restoreWebGlContext(context, false);
        }, 0);
    };
    const restoreWebGlContext = (context, explicit) => {
        const state = webGlState(context);
        if (!state.lost || !state.lossEventCompleted || !state.restoreAllowed ||
            (explicit && !state.simulatedLoss)) {
            webGlError(context, 0x0502); return;
        }
        if (state.restoreQueued) return;
        state.restoreQueued = true;
        const epoch = state.epoch;
        webGlQueueTask(() => {
            if (!state.lost || state.epoch !== epoch) return;
            state.restoreQueued = false;
            const attributes = state.attributes;
            const id = host('webglCreate', Math.max(1, state.canvas.width), Math.max(1, state.canvas.height),
                JSON.stringify({api:state.api, alpha:attributes.alpha, depth:attributes.depth,
                    stencil:attributes.stencil, antialias:attributes.antialias,
                    preserve:attributes.preserveDrawingBuffer}));
            // Native admission failure cannot expose a half-restored context.
            // An explicit extension request can be retried after releasing peers.
            if (!id) return;
            state.id = id;
            state.lost = false;
            state.lossReported = false;
            state.lossError = 0;
            state.simulatedLoss = false;
            state.unpackFlip = false;
            state.unpackPremultiply = false;
            state.unpackColorSpace = 0x9244;
            state.dirty = true;
            state.canvas.dispatchEvent(markTrusted(new WebGLContextEvent('webglcontextrestored', {cancelable:true})));
        }, 0);
    };
    class WEBGL_lose_context {
        constructor(token, context) {
            if (token !== webGlToken) throw new TypeError('Illegal constructor');
            webGlLoseExtensions.set(this, context);
        }
        loseContext() {
            const context = webGlLoseExtensions.get(this);
            if (!context) throw new TypeError('Illegal WEBGL_lose_context receiver');
            loseWebGlContext(context, true);
        }
        restoreContext() {
            const context = webGlLoseExtensions.get(this);
            if (!context) throw new TypeError('Illegal WEBGL_lose_context receiver');
            restoreWebGlContext(context, true);
        }
    }
    Object.defineProperty(WEBGL_lose_context.prototype, Symbol.toStringTag, {value:'WEBGL_lose_context'});
    const webGlExtension = (context, name) => {
        const state = webGlState(context);
        name = `${name}`.toLowerCase();
        if (state.lost) return null;
        if (name === 'webgl_lose_context') {
            if (!state.loseExtension) state.loseExtension = new WEBGL_lose_context(webGlToken, context);
            return state.loseExtension;
        }
        const canonical = webGlExtensionFactories.get(name);
        if (!canonical) return null;
        if (state.extensions.has(name)) return state.extensions.get(name);
        if (!webGlCall(context, 'enableExtension', [], [], canonical.name)) return null;
        const extension = canonical.create(context);
        state.extensions.set(name, extension);
        return extension;
    };
