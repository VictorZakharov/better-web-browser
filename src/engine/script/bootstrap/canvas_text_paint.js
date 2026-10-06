    // Solid paints and intrinsic color glyphs use the same native compositor as
    // other Canvas sources. Gradients/patterns retain the scalar paint resolver.
    const canvasPaintGlyphs = (context, state, glyphs, paint, x, y, scale, inverse, stroke) => {
        if (state.width*state.height < 256 || canvasIsGradient(paint) || canvasIsPattern(paint) ||
            !paint.channels) return false;
        const packet = [];
        let regionLeft=state.width,regionTop=state.height,regionRight=0,regionBottom=0;
        for (const [left,top,width,height,color,data] of glyphs) {
            const gx=x+left*scale, gy=y+top;
            const bounds=canvasTransformedBounds(canvasDrawingState(context).transform,
                gx,gy,width*scale,height,state);
            if (bounds) {
                packet.push([gx,gy,width,height,bounds,color,data]);
                regionLeft=Math.min(regionLeft,bounds[0]);regionTop=Math.min(regionTop,bounds[1]);
                regionRight=Math.max(regionRight,bounds[2]);regionBottom=Math.max(regionBottom,bounds[3]);
            }
        }
        if (!packet.length) return true;
        const regionWidth=regionRight-regionLeft,regionHeight=regionBottom-regionTop;
        if (!regionWidth||!regionHeight) return true;
        const region=new canvasPixelArray(regionWidth*regionHeight*4);
        for(let row=0;row<regionHeight;row++)
            copyCanvasPixelRow(region,row*regionWidth*4,state.pixels,
                ((row+regionTop)*state.width+regionLeft)*4,regionWidth*4);
        const painted=host('canvasPaintGlyphs',canvasPrivateWireStringify({
            width:state.width,height:state.height,
            region:[regionLeft,regionTop,regionRight,regionBottom],inverse,scale,
            opacity:canvasDrawingState(context).globalAlpha,
            operator:canvasDrawingState(context).compositeOperation,
            paint:paint.channels,opaque:canvasBitmapIsOpaque(state.pixels),stroke
        }),region,packet,canvasDrawingState(context).clipBits||null);
        if (!painted || canvasPixelLength(painted)!==region.length) return false;
        for(let row=0;row<regionHeight;row++)
            copyCanvasPixelRow(state.pixels,((row+regionTop)*state.width+regionLeft)*4,
                painted,row*regionWidth*4,regionWidth*4);
        return true;
    };
