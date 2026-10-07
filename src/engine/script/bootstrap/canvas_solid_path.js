    // The pure coverage mask remains native when it is immediately consumed by
    // a solid source-over paint. Read/write only the bounded destination region.
    const canvasPaintSolidPath = (context, state, kind, request, style,
        left, top, right, bottom) => {
        const width = right-left, height = bottom-top, pixels = width*height;
        if (pixels < 256 || pixels > MAX_CANVAS_PIXELS ||
            canvasDrawingState(context).compositeOperation !== 'source-over' || canvasIsGradient(style) ||
            canvasIsPattern(style) || !style.channels) return false;
        const region = new canvasPixelArray(pixels*4);
        for (let row=0;row<height;row++)
            copyCanvasPixelRow(region,row*width*4,state.pixels,
                ((row+top)*state.width+left)*4,width*4);
        const painted = canvasSolidMaskHost('canvasPaintSolidPath',kind,request,region,
            style.channels,canvasDrawingState(context).globalAlpha,
            canvasDrawingState(context).clipBits || null,state.width,state.height,left,top);
        if (!painted || canvasPixelLength(painted)!==region.length) return false;
        for (let row=0;row<height;row++)
            copyCanvasPixelRow(state.pixels,((row+top)*state.width+left)*4,
                painted,row*width*4,width*4);
        return true;
    };
