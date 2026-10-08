    // Keep pure coverage native when an existing source-over painter consumes
    // it immediately. Read/write only the bounded destination region.
    const canvasPaintNativePath = (context, state, kind, request, style,
        left, top, right, bottom, geometry) => {
        const width = right-left, height = bottom-top, pixels = width*height;
        if (pixels < 256 || pixels > MAX_CANVAS_PIXELS ||
            canvasDrawingState(context).compositeOperation !== 'source-over') return false;
        if (canvasIsGradient(style)) return canvasPaintGradientMask(context,state,null,style,
            left,top,right,bottom,kind,request,geometry);
        if (canvasIsPattern(style)) return canvasPaintPatternMask(context,state,null,style,
            left,top,right,bottom,kind,request,geometry);
        if (!style.channels) return false;
        return paintCanvasRegion(state, left, top, right, bottom,
            region => geometry ? canvasSolidMaskHost('canvasPaintSolidPath',kind,request,region,
            style.channels,canvasDrawingState(context).globalAlpha,
            canvasDrawingState(context).clipBits || null,state.width,state.height,left,top,geometry) :
            canvasSolidMaskHost('canvasPaintSolidPath',kind,request,region,
            style.channels,canvasDrawingState(context).globalAlpha,
            canvasDrawingState(context).clipBits || null,state.width,state.height,left,top));
    };
