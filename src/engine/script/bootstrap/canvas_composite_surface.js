    // Porter-Duff modes such as copy and source-in affect backdrop pixels *outside* the
    // painted shape. Render a bounded source layer first, then composite over the whole
    // clipped surface. Per-pixel blending inside a shape cannot implement these modes.
    const canvasNeedsSourceLayer = new Set(['copy', 'source-in', 'source-out',
        'source-atop', 'destination-in', 'destination-out', 'destination-atop', 'xor']);
    const canvasCompositeSourceLayer = (context, draw, args) => {
        const operator = context.__compositeOperation;
        const hasShadow = context.__shadowColor.channels[3] !== 0;
        const hasFilter = context.__filterOperations.length !== 0;
        if (!canvasNeedsSourceLayer.has(operator) && !hasShadow && !hasFilter) {
            if (draw === canvasOriginalDrawImage) paintCanvasImage.apply(context, args);
            else draw.apply(context, args);
            return;
        }
        const state = stateForCanvas(context.canvas);
        if (!state.pixels) return draw.apply(context, args);
        const destination = state.pixels;
        let source = new Uint8ClampedArray(destination.length);
        // Drawing a canvas into itself must read its original bitmap, not the temporary layer.
        if (draw === canvasOriginalDrawImage && args[0] === context.canvas) {
            args = [makeImageBitmap(state.width, state.height,
                new Uint8ClampedArray(destination)), ...args.slice(1)];
        }
        state.pixels = source;
        context.__compositeOperation = 'source-over';
        let painted = true;
        try {
            if (draw === canvasOriginalDrawImage) painted = paintCanvasImage.apply(context, args);
            else draw.apply(context, args);
        }
        finally {
            state.pixels = destination;
            context.__compositeOperation = operator;
        }
        if (!painted) return;
        if (hasFilter) source = applyCanvasFilters(source, state.width, state.height,
            context.__filterOperations);
        const layer = hasShadow ? canvasShadowLayer(context, source, state.width, state.height) : source;
        if (hasShadow) for (let offset = 0; offset < layer.length; offset += 4)
            compositeCanvasPixel(layer, offset, source.subarray(offset, offset + 4),
                1, 'source-over');
        for (let y = 0; y < state.height; y++) for (let x = 0; x < state.width; x++) {
            if (!canvasClipAllows(context, x, y, state.width)) continue;
            const offset = (y * state.width + x) * 4;
            compositeCanvasPixel(destination, offset,
                layer.subarray(offset, offset + 4), 1, operator);
        }
    };
    const canvasOriginalFillRect = CanvasRenderingContext2D.prototype.fillRect;
    const canvasOriginalFill = CanvasRenderingContext2D.prototype.fill;
    const canvasOriginalStroke = CanvasRenderingContext2D.prototype.stroke;
    const canvasOriginalDrawImage = CanvasRenderingContext2D.prototype.drawImage;
    CanvasRenderingContext2D.prototype.fillRect = function(...args) {
        const rect = normalizedRectangle(...args);
        if (!rect || !rect.width || !rect.height) return;
        return canvasCompositeSourceLayer(this, canvasOriginalFillRect, args);
    };
    CanvasRenderingContext2D.prototype.fill = function(...args) {
        const path = canvasPathArgument(this, args[0]);
        if (!path.subpaths.length) return;
        return canvasCompositeSourceLayer(this, canvasOriginalFill, args);
    };
    CanvasRenderingContext2D.prototype.stroke = function(...args) {
        const path = canvasPathArgument(this, args[0]);
        if (!path.subpaths.length) return;
        return canvasCompositeSourceLayer(this, canvasOriginalStroke, args);
    };
    CanvasRenderingContext2D.prototype.drawImage = function(...args) {
        return canvasCompositeSourceLayer(this, canvasOriginalDrawImage, args);
    };
    CanvasRenderingContext2D.prototype.strokeRect = function(x, y, width, height) {
        const path = new Path2D(); path.rect(x, y, width, height);
        this.stroke(path);
    };
