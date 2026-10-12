    // HTML draws the unclipped text source, then its shadow, then the source.
    // Keep the same sampling/compositing arithmetic but transfer only affected
    // destination rows. Filters and destructive operators use the dense path.
    const canvasPaintGlyphShadow = (context,state,glyphs,paint,x,y,scale,inverse,stroke) => {
        if (state.width*state.height < 256 || canvasIsGradient(paint) || canvasIsPattern(paint) ||
            !paint.channels) return false;
        const {packet,bounds} = canvasGlyphPacket(context,state,glyphs,x,y,scale);
        if (!packet.length) return true;
        const drawing = canvasDrawingState(context);
        const padding = canvasPrivateMath.ceil(canvasPrivateMath.min(drawing.shadowBlur,128)*1.5)+3;
        const axis = (start,end,offset,limit) => [
            canvasPrivateMath.max(0,canvasPrivateMath.min(start,canvasPrivateMath.floor(start-padding+offset-2))),
            canvasPrivateMath.min(limit,canvasPrivateMath.max(end,canvasPrivateMath.ceil(end+padding+offset+1)))
        ];
        const [left,right]=axis(bounds[0],bounds[2],drawing.shadowOffsetX,state.width);
        const [top,bottom]=axis(bounds[1],bounds[3],drawing.shadowOffsetY,state.height);
        const width=right-left,height=bottom-top;
        const destination=new canvasPrivatePixelArray(width*height*4);
        copyCanvasPixelRectangle(destination,0,width*4,state.pixels,(top*state.width+left)*4,
            state.width*4,width*4,height);
        const color=canvasPrivateColorBytes(drawing.shadowColor.channels);
        const request={glyph:{width:state.width,height:state.height,region:bounds,inverse,scale,
            opacity:drawing.globalAlpha,operator:drawing.compositeOperation,paint:paint.channels,
            opaque:canvasBitmapIsOpaque(state.pixels),stroke},destination:[left,top,right,bottom],
            blur:drawing.shadowBlur,offset:[drawing.shadowOffsetX,drawing.shadowOffsetY],
            color:[color[0],color[1],color[2],color[3]]};
        const encoded=canvasPrivateWireStringify(request);
        // Faint ordinary paint can also create hidden RGB through quantization.
        // Query the actual fixed bitmap without copying it, after preparation.
        // Dense composition still normalizes those pixels outside the ink.
        if (!request.glyph.opaque && host('canvasBitmapHasHiddenRgb',state.pixels)!==false) return false;
        const painted=host('canvasPaintGlyphShadow',encoded,
            destination,packet,drawing.clipBits||null);
        if (!painted || canvasPixelLength(painted)!==canvasPixelLength(destination)) return false;
        copyCanvasPixelRectangle(state.pixels,(top*state.width+left)*4,state.width*4,
            painted,0,width*4,width*4,height);
        return true;
    };
