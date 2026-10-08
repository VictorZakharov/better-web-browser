    // The pure coverage mask remains native when it is immediately consumed by
    // a solid source-over paint. Read/write only the bounded destination region.
    const canvasPaintSolidPath = (context, state, kind, request, style,
        left, top, right, bottom, geometry) => {
        const width = right-left, height = bottom-top, pixels = width*height;
        if (pixels < 256 || pixels > MAX_CANVAS_PIXELS ||
            canvasDrawingState(context).compositeOperation !== 'source-over' || canvasIsGradient(style) ||
            canvasIsPattern(style) || !style.channels) return false;
        return paintCanvasRegion(state, left, top, right, bottom,
            region => geometry ? canvasSolidMaskHost('canvasPaintSolidPath',kind,request,region,
            style.channels,canvasDrawingState(context).globalAlpha,
            canvasDrawingState(context).clipBits || null,state.width,state.height,left,top,geometry) :
            canvasSolidMaskHost('canvasPaintSolidPath',kind,request,region,
            style.channels,canvasDrawingState(context).globalAlpha,
            canvasDrawingState(context).clipBits || null,state.width,state.height,left,top));
    };
