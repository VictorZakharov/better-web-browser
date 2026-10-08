    const canvasPatternMaskHost=__hostCall;
    const canvasPaintPatternMask=(context,state,mask,style,left,top,right,bottom)=>{
        const pattern=canvasPatternGet(canvasPatternStates,style);
        if(!pattern||pattern.repetition!=='repeat'||pattern.source.pixels16)return false;
        const width=right-left,height=bottom-top,pixels=width*height;
        if(pixels<256||pixels>MAX_CANVAS_PIXELS||
            canvasDrawingState(context).compositeOperation!=='source-over'||pattern.source.width*pattern.source.height>1048576)return false;
        const request=canvasGradientMaskObject(null);
        request.width=width;request.height=height;request.left=left;request.top=top;
        request.source_width=pattern.source.width;request.source_height=pattern.source.height;
        request.transform=canvasGradientMaskNumbers(matrixMultiply2D(canvasDrawingState(context).transform,pattern.transform));
        request.opacity=canvasDrawingState(context).globalAlpha;
        const encoded=canvasGradientMaskStringify(request);
        return paintCanvasRegion(state,left,top,right,bottom,
            region=>canvasPatternMaskHost('canvasPaintPatternMask',encoded,region,mask,pattern.source.pixels,
                canvasDrawingState(context).clipBits||null,state.width,state.height,left,top));
    };
