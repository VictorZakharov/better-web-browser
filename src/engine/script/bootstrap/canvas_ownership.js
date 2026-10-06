    // Bitmap dimensions and detached state are platform data. Internal drawing
    // must not call an author's shadowed width/height/getAttribute accessors.
    const canvasOwnerWeakGet = Function.call.bind(WeakMap.prototype.get);
    const canvasOwnerWeakSet = Function.call.bind(WeakMap.prototype.set);
    const offscreenCanvasDimensions = new WeakMap();
    const canvasNativeElement = (canvas, localName) => {
        if (typeof nativeNodeMetadata === 'undefined') return false;
        const metadata = canvasOwnerWeakGet(nativeNodeMetadata, canvas);
        return metadata?.type === 1 && metadata.localName === localName &&
            metadata.namespaceURI === 'http://www.w3.org/1999/xhtml';
    };
    const canvasHtmlReceiver = canvas => {
        if (!canvasNativeElement(canvas, 'canvas')) throw new TypeError('Illegal canvas receiver');
        return canvas;
    };
    const canvasOwnerAttribute = (canvas, name) => {
        canvasHtmlReceiver(canvas);
        return __hostCall('attrGetNs', canvasOwnerWeakGet(nodeHandles, canvas), '', name);
    };
    const canvasOffscreenDimensions = canvas => {
        const dimensions = canvasOwnerWeakGet(offscreenCanvasDimensions, canvas);
        if (!dimensions) throw new TypeError('Illegal OffscreenCanvas receiver');
        return dimensions;
    };
    const canvasOffscreenDetached = canvas =>
        canvasOwnerWeakGet(offscreenCanvasDimensions, canvas)?.detached === true;
    const canvasOwnedDimensions = canvas => {
        const dimensions = canvasOwnerWeakGet(offscreenCanvasDimensions, canvas);
        if (dimensions) return dimensions.detached ? [0, 0] : [dimensions.width, dimensions.height];
        return [canvasDimension(canvas, 'width', 300), canvasDimension(canvas, 'height', 150)];
    };
    const canvasSetAttributeNS = typeof Element === 'undefined' ? null :
        Function.call.bind(Element.prototype.setAttributeNS);
    const setCanvasDimension = (canvas, name, value, fallback) => {
        canvasHtmlReceiver(canvas);
        const converted = +value >>> 0;
        canvasSetAttributeNS(canvas, null, name, String(converted > 2147483647 ? fallback : converted));
    };
    // DOM attribute steps, not just property setters, resize/reset Canvas.
    // Install only in the Window bootstrap; workers have no Element bindings.
    if (typeof validateCanvasAttributeSet !== 'undefined') {
        validateCanvasAttributeSet = (canvas, namespace, name) => {
            if (namespace !== null || (name !== 'width' && name !== 'height') ||
                !canvasNativeElement(canvas, 'canvas')) return;
            if (canvasOwnerWeakGet(canvasStates, canvas)?.mode === 'placeholder')
                throw new DOMException('Canvas is controlled by an OffscreenCanvas', 'InvalidStateError');
        };
        canvasAttributeChanged = (canvas, namespace, name) => {
            if (namespace !== null || (name !== 'width' && name !== 'height') ||
                !canvasNativeElement(canvas, 'canvas')) return;
            const state = canvasOwnerWeakGet(canvasStates, canvas);
            if (!state || state.mode === 'placeholder') return;
            // Bitmaprenderer retains a transferred output bitmap until replaced;
            // resizing only resets its output while the bitmap mode is blank.
            if (state.mode === 'bitmaprenderer' && canvasOwnerWeakGet(bitmapRendererStates, state.context)?.blank === false) {
                [state.inputWidth, state.inputHeight] = canvasOwnedDimensions(canvas);
                return;
            }
            stateForCanvas(canvas, true);
        };
    }
