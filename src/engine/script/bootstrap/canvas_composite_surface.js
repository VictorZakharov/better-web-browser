    // Porter-Duff modes such as copy and source-in affect backdrop pixels *outside* the
    // painted shape. Render a bounded source layer first, then composite over the whole
    // clipped surface. Per-pixel blending inside a shape cannot implement these modes.
    const canvasNeedsSourceLayer = new Set(['copy', 'source-in', 'source-out',
        'source-atop', 'destination-in', 'destination-out', 'destination-atop', 'xor']);
    const canvasCompositeLayerHost = __hostCall;
    const canvasCompositeLayer = (context, destination, layer, width, height, operator) => {
        if (width * height >= 256 && width * height <= MAX_CANVAS_PIXELS) {
            const painted = canvasCompositeLayerHost('canvasCompositeLayer',
                destination, layer, operator, canvasDrawingState(context).clipBits || null,
                canvasBitmapIsOpaque(destination));
            if (painted && canvasPixelLength(painted) === canvasPrivateCount(destination)) {
                copyCanvasPixelRow(destination, 0, painted, 0, canvasPrivateCount(destination));
                return;
            }
        }
        for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
            if (!canvasClipAllows(context, x, y, width)) continue;
            const offset = (y * width + x) * 4;
            compositeCanvasPixelAt(destination, offset, layer, offset, 1, operator);
        }
    };
    const canvasCompositeSourceLayer = (context, draw, args) => {
        const operator = canvasDrawingState(context).compositeOperation;
        const hasShadow = canvasDrawingState(context).shadowColor.channels[3] !== 0 &&
            (canvasDrawingState(context).shadowBlur !== 0 || canvasDrawingState(context).shadowOffsetX !== 0 || canvasDrawingState(context).shadowOffsetY !== 0);
        const hasFilter = canvasDrawingState(context).filterOperations.length !== 0;
        if (!canvasNeedsSourceLayer.has(operator) && !hasShadow && !hasFilter) {
            if (draw === canvasOriginalDrawImage) paintCanvasImage.apply(context, args);
            else draw.apply(context, args);
            return;
        }
        const state = stateForCanvas(context.canvas);
        if (!state.pixels) return draw.apply(context, args);
        if (hasShadow && !hasFilter && state.width*state.height >= 256 &&
            canvasPaintShadowPath(context,draw,args,state)) return;
        const destination = state.pixels;
        const settings = canvasDrawingState(context);
        const width = state.width, height = state.height;
        const halo = hasFilter ? canvasFilterHalo(settings) : 0;
        const sourceWidth = width + halo * 2, sourceHeight = height + halo * 2;
        if (!Number.isSafeInteger(halo) || sourceWidth > 16384 || sourceHeight > 16384 ||
            sourceWidth * sourceHeight > MAX_CANVAS_PIXELS)
            throw new DOMException('Canvas filter source exceeds the bounded working set',
                'NotSupportedError');
        let source = new canvasPrivatePixelArray(sourceWidth * sourceHeight * 4);
        // Drawing a canvas into itself must read its original bitmap, not the temporary layer.
        if (draw === canvasOriginalDrawImage && args[0] === context.canvas) {
            args = [makeImageBitmap(state.width, state.height,
                new canvasPrivatePixelArray(destination), false, null, state.originClean !== false), ...args.slice(1)];
        }
        state.pixels = source;
        state.width = sourceWidth; state.height = sourceHeight;
        const savedTransform = settings.transform, savedPath = settings.path;
        if (halo) {
            settings.transform = matrixMultiply2D([1,0,0,1,halo,halo], savedTransform);
            settings.path = transformCanvasPath(savedPath, [1,0,0,1,halo,halo]);
        }
        const savedClip = canvasDrawingState(context).clipBits;
        // The source and shadow are generated before the drawing clip is applied.
        canvasDrawingState(context).clipBits = null;
        canvasDrawingState(context).compositeOperation = 'source-over';
        let painted = true;
        try {
            if (draw === canvasOriginalDrawImage) painted = paintCanvasImage.apply(context, args);
            else draw.apply(context, args);
        }
        finally {
            state.pixels = destination;
            state.width = width; state.height = height;
            settings.transform = savedTransform; settings.path = savedPath;
            canvasDrawingState(context).clipBits = savedClip;
            canvasDrawingState(context).compositeOperation = operator;
        }
        if (!painted) return;
        if (hasFilter) source = applyCanvasFilters(source, sourceWidth, sourceHeight,
            canvasDrawingState(context).filterOperations);
        // Filter Effects flags currentColor-dependent primitives as tainted.
        // This belongs to bitmap ownership, not the saved drawing-state stack.
        if (hasFilter) for (let index = 0; index < settings.filterOperations.length; index++) {
            const operation = settings.filterOperations[index];
            if (operation.name === 'drop-shadow' && operation.value.originClean === false)
                state.originClean = false;
        }
        if (!halo && hasShadow && state.width * state.height >= 256) {
            const painted = canvasCompositeLayerHost('canvasPaintSourceLayer', destination, source,
                operator, savedClip || null, state.width, state.height, canvasDrawingState(context).shadowBlur,
                canvasDrawingState(context).shadowOffsetX, canvasDrawingState(context).shadowOffsetY,
                canvasPrivateColorBytes(canvasDrawingState(context).shadowColor.channels), canvasBitmapIsOpaque(destination));
            if (painted && canvasPixelLength(painted) === canvasPrivateCount(destination)) {
                copyCanvasPixelRow(destination, 0, painted, 0, canvasPrivateCount(destination));
                return;
            }
        }
        // HTML's drawing model composites the shadow and source separately using
        // the selected operator. Flattening them with source-over breaks source-in,
        // destination-in, copy and the other non-associative Porter-Duff modes.
        if (hasShadow) {
            let shadow = canvasShadowLayer(context, source, sourceWidth, sourceHeight);
            shadow = canvasFilterCrop(shadow, sourceWidth, sourceHeight, halo, width, height);
            canvasCompositeLayer(context, destination, shadow, state.width, state.height, operator);
        }
        source = canvasFilterCrop(source, sourceWidth, sourceHeight, halo, width, height);
        canvasCompositeLayer(context, destination, source, state.width, state.height, operator);
    };
    const canvasOriginalFillRect = CanvasRenderingContext2D.prototype.fillRect;
    const canvasOriginalFill = CanvasRenderingContext2D.prototype.fill;
    const canvasOriginalStroke = CanvasRenderingContext2D.prototype.stroke;
    const canvasOriginalDrawImage = CanvasRenderingContext2D.prototype.drawImage;
    CanvasRenderingContext2D.prototype.fillRect = function(...args) {
        const rect = normalizedRectangle(...args);
        if (!rect || !rect.width || !rect.height) return;
        // Web IDL conversions happen once, before the drawing operation. Reusing
        // converted coordinates avoids calling author valueOf twice for fillRect.
        return canvasCompositeSourceLayer(this, canvasOriginalFillRect,
            [rect.x, rect.y, rect.width, rect.height]);
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
        const converted = canvasImageDrawArguments(this, args);
        if (!converted.slice(1).every(Number.isFinite)) return;
        return canvasCompositeSourceLayer(this, canvasOriginalDrawImage, converted);
    };
    CanvasRenderingContext2D.prototype.strokeRect = function(x, y, width, height) {
        const path = new Path2D(); path.rect(x, y, width, height);
        this.stroke(path);
    };
