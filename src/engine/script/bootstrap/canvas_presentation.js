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
        for (const canvas of presentedCanvases) {
            if (canvas.isConnected) continue;
            detachedCanvases.add(new WeakRef(canvas));
            presentedCanvases.delete(canvas);
            dirtyCanvases.delete(canvas);
            presentedDimensions.delete(canvas);
            exportedBitmaps.delete(canvas);
        }
        while (detachedCanvases.size > MAX_PRESENTED_CANVASES)
            detachedCanvases.delete(detachedCanvases.values().next().value);
    };
    const restoreReattachedCanvases = () => {
        for (const reference of detachedCanvases) {
            const canvas = reference.deref();
            if (!canvas) { detachedCanvases.delete(reference); continue; }
            if (!canvas.isConnected || presentedCanvases.size >= MAX_PRESENTED_CANVASES) continue;
            presentedCanvases.add(canvas);
            dirtyCanvases.add(canvas);
            detachedCanvases.delete(reference);
        }
    };

    const dirtyCanvas = surface => {
        const canvas = surface instanceof OffscreenCanvas
            ? placeholderOwners.get(surface) : surface;
        if (!(canvas instanceof HTMLCanvasElement)) return;
        if (!presentedCanvases.has(canvas) && presentedCanvases.size >= MAX_PRESENTED_CANVASES) {
            forgetDetachedCanvases();
            if (presentedCanvases.size >= MAX_PRESENTED_CANVASES) return;
        }
        presentedCanvases.add(canvas);
        if (dirtyCanvases.has(canvas)) return;
        dirtyCanvases.add(canvas);
        host('canvasPaintDirty', nodeId(canvas));
    };
    const paintMethod = (prototype, name, owner = receiver => receiver.canvas) => {
        const descriptor = Object.getOwnPropertyDescriptor(prototype, name);
        if (typeof descriptor?.value !== 'function') return;
        const original = descriptor.value;
        const wrapped = function(...args) {
            const result = Reflect.apply(original, this, args);
            dirtyCanvas(owner(this));
            return result;
        };
        Object.defineProperties(wrapped, {
            length: { value: original.length }, name: { value: original.name }
        });
        Object.defineProperty(prototype, name, { ...descriptor, value: wrapped });
    };
    for (const name of ['clearRect', 'fillRect', 'strokeRect', 'putImageData',
        'fill', 'stroke', 'drawImage', 'fillText', 'strokeText', 'drawFocusIfNeeded', 'reset'])
        paintMethod(CanvasRenderingContext2D.prototype, name);
    paintMethod(ImageBitmapRenderingContext.prototype, 'transferFromImageBitmap');
    paintMethod(OffscreenCanvas.prototype, 'transferToImageBitmap', canvas => canvas);

    const originalGetContext = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = function(...args) {
        const context = Reflect.apply(originalGetContext, this, args);
        if (context && !presentedCanvases.has(this)) dirtyCanvas(this);
        return context;
    };
    Object.defineProperty(HTMLCanvasElement.prototype.getContext, 'length',
        { value: originalGetContext.length });
    for (const name of ['width', 'height']) {
        const descriptor = Object.getOwnPropertyDescriptor(HTMLCanvasElement.prototype, name);
        Object.defineProperty(HTMLCanvasElement.prototype, name, {
            ...descriptor,
            set(value) {
                descriptor.set.call(this, value);
                if (presentedCanvases.has(this)) dirtyCanvas(this);
            }
        });
        const offscreenDescriptor = Object.getOwnPropertyDescriptor(OffscreenCanvas.prototype, name);
        Object.defineProperty(OffscreenCanvas.prototype, name, {
            ...offscreenDescriptor,
            set(value) {
                offscreenDescriptor.set.call(this, value);
                dirtyCanvas(this);
            }
        });
    }
    const originalTransfer = HTMLCanvasElement.prototype.transferControlToOffscreen;
    HTMLCanvasElement.prototype.transferControlToOffscreen = function(...args) {
        const offscreen = Reflect.apply(originalTransfer, this, args);
        placeholderOwners.set(offscreen, this);
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
        const ordered = [...presentedCanvases];
        // A repainting earlier Canvas must not starve one whose pixels have
        // never crossed this export boundary.
        ordered.sort((left, right) => Number(exportedBitmaps.has(left)) -
            Number(exportedBitmaps.has(right)));
        for (const canvas of ordered) {
            const width = canvas.width, height = canvas.height;
            const previous = presentedDimensions.get(canvas);
            if (previous?.[0] !== width || previous?.[1] !== height) {
                stateForCanvas(canvas);
                dirtyCanvases.add(canvas);
            }
            if (!dirtyCanvases.has(canvas)) continue;
            const state = stateForCanvas(canvas);
            const placeholder = state.placeholder;
            const output = placeholder && !placeholder.__detached
                ? stateForCanvas(placeholder) : state;
            let pixels = output.pixels;
            if (placeholder?.__detached || output.width !== width || output.height !== height ||
                !width || !height || !pixels)
                pixels = null;
            else if (pixels.byteLength > MAX_PRESENTED_CANVAS_BYTES) {
                // The atomic bitmap cannot fit in any batch. Keep it dirty but
                // do not spin zero-delay checkpoints for an impossible export.
                continue;
            }
            else if (bytes + pixels.byteLength > MAX_PRESENTED_CANVAS_BYTES) {
                // Null means "remove the current bitmap" to the renderer. Keep
                // both pixels and the dirty marker for the next bounded batch.
                if (!deferred) host('canvasPaintDirty', nodeId(canvas));
                deferred = true;
                continue;
            } else bytes += pixels.byteLength;
            snapshots.push([nodeId(canvas), width, height, pixels]);
            if (pixels) exportedBitmaps.add(canvas);
            presentedDimensions.set(canvas, [width, height]);
            dirtyCanvases.delete(canvas);
        }
        return snapshots;
    };
