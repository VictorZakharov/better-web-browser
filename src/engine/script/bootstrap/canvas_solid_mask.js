    // Geometry and painting stay separate. Batch only solid source-over regions;
    // gradients, patterns, clipping and other operators retain the scalar path.
    const canvasSolidMaskHost = __hostCall;
    const canvasPaintSolidMask = (context, state, mask, style, left, top, right, bottom) => {
        const width = right - left, height = bottom - top, pixels = width * height;
        if (pixels < 256 || pixels > MAX_CANVAS_PIXELS || context.__clipBits ||
            canvasIsGradient(style) || style instanceof CanvasPattern ||
            context.__compositeOperation !== 'source-over' || !style.channels ||
            canvasPixelLength(mask) !== pixels) return false;
        const region = new canvasPixelArray(pixels * 4);
        for (let row = 0; row < height; row++)
            copyCanvasPixelRow(region, row * width * 4, state.pixels,
                ((row + top) * state.width + left) * 4, width * 4);
        const painted = canvasSolidMaskHost('canvasPaintSolidMask', mask, region,
            style.channels, context.__globalAlpha);
        if (!painted || canvasPixelLength(painted) !== region.length) return false;
        for (let row = 0; row < height; row++)
            copyCanvasPixelRow(state.pixels, ((row + top) * state.width + left) * 4,
                painted, row * width * 4, width * 4);
        return true;
    };
