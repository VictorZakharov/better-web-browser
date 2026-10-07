    // Reuse the same clipped bitmap bounds and tiny-skia request as ordinary
    // solid path painting. Filters/patterns keep the existing source-layer path.
    const canvasPaintShadowPath = (context, draw, args, state) => {
        if (draw !== canvasOriginalFill && draw !== canvasOriginalStroke) return false;
        const drawing = canvasDrawingState(context), fill = draw === canvasOriginalFill;
        const style = fill ? drawing.fill : drawing.stroke;
        if (canvasIsGradient(style) || canvasIsPattern(style) || !style.channels) return false;
        const path = canvasPathArgument(context, args[0]);
        const inverse = matrixInverse2D(drawing.transform);
        if (!fill && !inverse) return false;
        const [a,b,c,d] = drawing.transform;
        const scale = Math.max(Math.hypot(a,c),Math.hypot(b,d));
        const inset = fill ? 0 : drawing.lineWidth/2*scale*
            (drawing.lineJoin === 'miter' ? drawing.miterLimit : 1)+1;
        const [left,top,right,bottom] = canvasPixelBounds(path,state,inset);
        if (!(left<right && top<bottom)) return false;
        const request = fill ? canvasRasterStringify({width:right-left,height:bottom-top,left,top,
            rule:canvasFillRule(canvasPathData.has(args[0]) ? args[1] : args[0]),
            parts:path.subpaths.map(part=>({points:part.points,closed:!!part.closed}))}) :
            canvasNativeStrokeRequest(context,transformCanvasPath(path,inverse),right-left,bottom-top,left,top);
        const painted = canvasCompositeLayerHost('canvasPaintShadowPath',state.pixels,request,
            drawing.compositeOperation,drawing.clipBits || null,state.width,state.height,
            drawing.shadowBlur,drawing.shadowOffsetX,drawing.shadowOffsetY,
            new Uint8Array(drawing.shadowColor.channels),canvasBitmapIsOpaque(state.pixels),
            fill ? 'fill' : 'stroke',style.channels,drawing.globalAlpha);
        if (!painted || canvasPixelLength(painted)!==state.pixels.length) return false;
        copyCanvasPixelRow(state.pixels,0,painted,0,state.pixels.length);
        return true;
    };
