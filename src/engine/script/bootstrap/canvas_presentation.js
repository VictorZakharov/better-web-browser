    // The paint checkpoint snapshots only dirty, connected HTML canvases. This hook is
    // captured privately by the embedder before author script runs.
    // https://html.spec.whatwg.org/multipage/canvas.html#the-canvas-element
    const MAX_PRESENTED_CANVASES = 64;
    const MAX_PRESENTED_CANVAS_BYTES = 64 * 1024 * 1024;
    const presentedCanvases = new Set();
    const dirtyCanvases = new Set();
    // Detached canvases must not be retained just to notice a later reattach.
    // Weak references let a connected surface rejoin the next paint checkpoint.
    const detachedCanvases = new Set();
    const presentedDimensions = new WeakMap();
    const exportedBitmaps = new WeakSet();
    const placeholderOwners = new WeakMap();

    const forgetDetachedCanvases = () => {
        canvasPrivateSetEach(presentedCanvases, canvas => {
            if (canvas.isConnected) return;
            canvasPrivateSetAdd(detachedCanvases, new canvasPrivateWeakRef(canvas));
            canvasPrivateSetDelete(presentedCanvases, canvas);
            canvasPrivateSetDelete(dirtyCanvases, canvas);
            canvasPrivateWeakDelete(presentedDimensions, canvas);
            canvasPrivateWeakSetDelete(exportedBitmaps, canvas);
        });
        while (canvasPrivateSetSize(detachedCanvases) > MAX_PRESENTED_CANVASES) {
            let first;
            canvasPrivateSetEach(detachedCanvases, reference => { if (!first) first = reference; });
            canvasPrivateSetDelete(detachedCanvases, first);
        }
    };
    const restoreReattachedCanvases = () => {
        canvasPrivateSetEach(detachedCanvases, reference => {
            const canvas = canvasPrivateDeref(reference);
            if (!canvas) { canvasPrivateSetDelete(detachedCanvases, reference); return; }
            if (!canvas.isConnected || canvasPrivateSetSize(presentedCanvases) >= MAX_PRESENTED_CANVASES) return;
            canvasPrivateSetAdd(presentedCanvases, canvas);
            canvasPrivateSetAdd(dirtyCanvases, canvas);
            canvasPrivateSetDelete(detachedCanvases, reference);
        });
    };

    const dirtyCanvas = surface => {
        const canvas = surface instanceof OffscreenCanvas
            ? canvasPrivateWeakGet(placeholderOwners, surface) : surface;
        if (!(canvas instanceof HTMLCanvasElement)) return;
        if (!canvasPrivateSetHas(presentedCanvases, canvas) && canvasPrivateSetSize(presentedCanvases) >= MAX_PRESENTED_CANVASES) {
            forgetDetachedCanvases();
            if (canvasPrivateSetSize(presentedCanvases) >= MAX_PRESENTED_CANVASES) return;
        }
        canvasPrivateSetAdd(presentedCanvases, canvas);
        if (canvasPrivateSetHas(dirtyCanvases, canvas)) return;
        canvasPrivateSetAdd(dirtyCanvases, canvas);
        host('canvasPaintDirty', nodeId(canvas));
    };
    const paintMethod = (prototype, name, owner = receiver => receiver.canvas) => {
        const descriptor = Object.getOwnPropertyDescriptor(prototype, name);
        if (typeof descriptor?.value !== 'function') return;
        const original = descriptor.value;
        const wrapped = function(...args) {
            const result = canvasPrivateApply(original, this, args);
            dirtyCanvas(owner(this));
            return result;
        };
        Object.defineProperties(wrapped, {
            length: { value: original.length }, name: { value: original.name }
        });
        Object.defineProperty(prototype, name, { ...descriptor, value: wrapped });
    };
    // Extension draws use private context brands and are not methods of the
    // WebGLRenderingContext prototype. They still schedule the same paint checkpoint.
    dirtyWebGlCanvas = context => dirtyCanvas(webGlState(context).canvas);
    for (const name of ['clearRect', 'fillRect', 'strokeRect', 'putImageData',
        'fill', 'stroke', 'drawImage', 'fillText', 'strokeText', 'drawFocusIfNeeded', 'reset'])
        paintMethod(CanvasRenderingContext2D.prototype, name);
    paintMethod(ImageBitmapRenderingContext.prototype, 'transferFromImageBitmap');
    for (const name of ['clear', 'drawArrays', 'drawElements'])
        paintMethod(WebGLRenderingContext.prototype, name);
    for (const name of ['clear','drawArrays','drawElements','drawArraysInstanced',
        'drawElementsInstanced','drawRangeElements','blitFramebuffer',
        'clearBufferfv','clearBufferiv','clearBufferuiv','clearBufferfi'])
        paintMethod(webGl2Prototype,name);
    paintMethod(OffscreenCanvas.prototype, 'transferToImageBitmap', canvas => canvas);

    const originalGetContext = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = function(...args) {
        const context = canvasPrivateApply(originalGetContext, this, args);
        if (context && !canvasPrivateSetHas(presentedCanvases, this)) dirtyCanvas(this);
        return context;
    };
    Object.defineProperty(HTMLCanvasElement.prototype.getContext, 'length',
        { value: originalGetContext.length });
    for (const name of ['width', 'height']) {
        const descriptor = Object.getOwnPropertyDescriptor(HTMLCanvasElement.prototype, name);
        Object.defineProperty(HTMLCanvasElement.prototype, name, {
            ...descriptor,
            set(value) {
                canvasPrivateApply(descriptor.set, this, [value]);
                if (canvasPrivateSetHas(presentedCanvases, this)) dirtyCanvas(this);
            }
        });
        const offscreenDescriptor = Object.getOwnPropertyDescriptor(OffscreenCanvas.prototype, name);
        Object.defineProperty(OffscreenCanvas.prototype, name, {
            ...offscreenDescriptor,
            set(value) {
                canvasPrivateApply(offscreenDescriptor.set, this, [value]);
                dirtyCanvas(this);
            }
        });
    }
    const originalTransfer = HTMLCanvasElement.prototype.transferControlToOffscreen;
    HTMLCanvasElement.prototype.transferControlToOffscreen = function(...args) {
        const offscreen = canvasPrivateApply(originalTransfer, this, args);
        canvasPrivateWeakSet(placeholderOwners, offscreen, this);
        dirtyCanvas(this);
        return offscreen;
    };
    Object.defineProperty(HTMLCanvasElement.prototype.transferControlToOffscreen, 'length',
        { value: originalTransfer.length });

    globalThis.__takeCanvasPresentation = () => {
        const snapshots = [];
        let bytes = 0;
        let deferred = false;
        forgetDetachedCanvases();
        restoreReattachedCanvases();
        const ordered = [];
        canvasPrivateSetEach(presentedCanvases, canvas => canvasPrivatePush(ordered, canvas));
        // A repainting earlier Canvas must not starve one whose pixels have
        // never crossed this export boundary.
        canvasPrivateSort(ordered, (left, right) => (canvasPrivateWeakSetHas(exportedBitmaps, left) ? 1 : 0) -
            (canvasPrivateWeakSetHas(exportedBitmaps, right) ? 1 : 0));
        for (let index = 0; index < ordered.length; index++) {
            const canvas = ordered[index];
            const dimensions = canvasOwnedDimensions(canvas);
            const width = dimensions[0], height = dimensions[1];
            const previous = canvasPrivateWeakGet(presentedDimensions, canvas);
            if (previous?.[0] !== width || previous?.[1] !== height) {
                stateForCanvas(canvas);
                canvasPrivateSetAdd(dirtyCanvases, canvas);
            }
            if (!canvasPrivateSetHas(dirtyCanvases, canvas)) continue;
            // Draw/clear wrappers conservatively schedule a checkpoint, including
            // FBO-only passes. Ask the native owner once here, after its ordered
            // command batch, rather than querying on every draw or trusting a
            // JavaScript copy of framebuffer bindings. Export APIs still read
            // the actual (possibly implicitly cleared) drawing buffer.
            const cached = canvasPrivateWeakGet(canvasStates, canvas);
            const backing = cached?.placeholder && !canvasOffscreenDetached(cached.placeholder)
                ? canvasPrivateWeakGet(canvasStates, cached.placeholder) : cached;
            if (canvasPrivateWeakSetHas(exportedBitmaps, canvas) && previous?.[0] === width && previous?.[1] === height &&
                backing?.mode === 'webgl') {
                const native = webGlContexts.get(backing.context);
                if (native && !native.lost && webGlCall(backing.context, 'drawingBufferDirty') === false) {
                    canvasPrivateSetDelete(dirtyCanvases, canvas);
                    continue;
                }
            }
            const state = stateForCanvas(canvas);
            const placeholder = state.placeholder;
            const output = placeholder && !canvasOffscreenDetached(placeholder)
                ? stateForCanvas(placeholder) : state;
            let pixels = output.pixels;
            if (canvasOffscreenDetached(placeholder) || !output.width || !output.height ||
                !width || !height || !pixels)
                pixels = null;
            else if (canvasPrivateByteLength(pixels) > MAX_PRESENTED_CANVAS_BYTES) {
                // The atomic bitmap cannot fit in any batch. Keep it dirty but
                // do not spin zero-delay checkpoints for an impossible export.
                continue;
            }
            else if (bytes + canvasPrivateByteLength(pixels) > MAX_PRESENTED_CANVAS_BYTES) {
                // Null means "remove the current bitmap" to the renderer. Keep
                // both pixels and the dirty marker for the next bounded batch.
                if (!deferred) host('canvasPaintDirty', nodeId(canvas));
                deferred = true;
                continue;
            } else bytes += canvasPrivateByteLength(pixels);
            // The backing bitmap may have different natural dimensions after
            // bitmaprenderer transfer. Preserve both it and the attribute-size
            // stamp, so native painting can reject stale post-resize assets.
            canvasPrivatePush(snapshots, [nodeId(canvas), output.width, output.height, width, height, pixels]);
            if (pixels && output.mode === 'webgl') webGlPresented(output);
            if (pixels) canvasPrivateWeakSetAdd(exportedBitmaps, canvas);
            canvasPrivateWeakSet(presentedDimensions, canvas, [width, height]);
            canvasPrivateSetDelete(dirtyCanvases, canvas);
        }
        return snapshots;
    };
