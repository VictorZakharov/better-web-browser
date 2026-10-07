    // Geometry and painting stay separate. Bounded source-over shaders batch;
    // unsupported patterns and other operators retain the scalar path.
    const canvasSolidMaskHost = __hostCall;
    const canvasPaintSolidMask = (context, state, mask, style, left, top, right, bottom) => {
        const width = right - left, height = bottom - top, pixels = width * height;
        if (canvasIsGradient(style))
            return canvasPaintGradientMask(context, state, mask, style, left, top, right, bottom);
        if (canvasIsPattern(style))
            return canvasPaintPatternMask(context, state, mask, style, left, top, right, bottom);
        if (pixels < 256 || pixels > MAX_CANVAS_PIXELS ||
            canvasIsGradient(style) || canvasIsPattern(style) ||
            canvasDrawingState(context).compositeOperation !== 'source-over' || !style.channels ||
            mask !== null && canvasPixelLength(mask) !== pixels) return false;
        const region = new canvasPixelArray(pixels * 4);
        for (let row = 0; row < height; row++)
            copyCanvasPixelRow(region, row * width * 4, state.pixels,
                ((row + top) * state.width + left) * 4, width * 4);
        const painted = canvasSolidMaskHost('canvasPaintSolidMask', mask, region,
            style.channels, canvasDrawingState(context).globalAlpha,
            canvasDrawingState(context).clipBits || null,state.width,state.height,left,top,width,height);
        if (!painted || canvasPixelLength(painted) !== region.length) return false;
        for (let row = 0; row < height; row++)
            copyCanvasPixelRow(state.pixels, ((row + top) * state.width + left) * 4,
                painted, row * width * 4, width * 4);
        return true;
    };
