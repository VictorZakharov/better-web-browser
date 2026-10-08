    // Private normalized source words. Canvas presentation stays RGBA8; only
    // genuinely higher-precision decoded sources receive this owned sidecar.
    // HTML ImageBitmap copies/transfers bitmap data, including source precision.
    // https://html.spec.whatwg.org/multipage/imagebitmap-and-animations.html#imagebitmap
    const preciseBitmapWords = Uint16Array, preciseBitmapBytes = Uint8Array;
    const preciseBitmapDataView = DataView;
    const preciseBitmapReadWord = Function.call.bind(DataView.prototype.getUint16);
    const preciseBitmapWriteWord = Function.call.bind(DataView.prototype.setUint16);
    const bitmapPrecisionBudget = (source,width,height) => {
        if (!source.pixels16) return;
        bitmapPixelBudget(width,height);
        // Source + destination word/byte views, with an equal allowance for
        // snapshots and serialization. Reject before allocating, not afterwards.
        const bytes=canvasPrivateByteLength(source.pixels16)+canvasPrivateByteLength(source.pixels)+width*height*12;
        if (!Number.isSafeInteger(bytes) || bytes*2>64*1024*1024)
            throw new DOMException('Precise bitmap exceeds the working budget','NotSupportedError');
    };
    const decodedBitmapWords = bytes => {
        if (!bytes) return null;
        const words=new preciseBitmapWords(canvasPrivateByteLength(bytes)/2);
        const view=new preciseBitmapDataView(canvasPrivateBuffer(bytes),canvasPrivateOffset(bytes),canvasPrivateByteLength(bytes));
        for(let index=0;index<canvasPrivateCount(words);index++) words[index]=preciseBitmapReadWord(view,index*2,true);
        return words;
    };
    const encodedBitmapWords = words => {
        if (!words) return null;
        const bytes=new preciseBitmapBytes(canvasPrivateCount(words)*2),view=new preciseBitmapDataView(canvasPrivateBuffer(bytes));
        for(let index=0;index<canvasPrivateCount(words);index++) preciseBitmapWriteWord(view,index*2,words[index],true);
        return bytes;
    };
    const narrowBitmapWords = words => {
        const pixels=new canvasPrivatePixelArray(canvasPrivateCount(words));
        for(let index=0;index<canvasPrivateCount(words);index++) pixels[index]=canvasPrivateMath.floor((words[index]+128)/257);
        return pixels;
    };
    const copyBitmapWords = state => {
        if (!state.pixels16) return null;
        bitmapPrecisionBudget(state,state.width,state.height);
        const words=new preciseBitmapWords(state.pixels16);
        if (state.premultiplied) for(let offset=0;offset<canvasPrivateCount(words);offset+=4) {
            const alpha=words[offset+3];
            for(let channel=0;channel<3;channel++) words[offset+channel]=alpha===0?0:
                canvasPrivateMath.min(65535,canvasPrivateMath.round(words[offset+channel]*65535/alpha));
        }
        return words;
    };
    const cropBitmapWords = (source,x,y,width,height) => {
        if (!source.pixels16) return null;
        bitmapPrecisionBudget(source,width,height);
        const words=new preciseBitmapWords(width*height*4);
        for(let row=0;row<height;row++) for(let column=0;column<width;column++) {
            const fromX=x+column,fromY=y+row;
            if(fromX<0||fromY<0||fromX>=source.width||fromY>=source.height)continue;
            const input=(fromY*source.width+fromX)*4;
            canvasPrivateTypedSet(words, canvasPrivateView(source.pixels16,input,input+4),(row*width+column)*4);
        }
        return words;
    };
    const flipBitmapWords = source => {
        if (!source.pixels16) return null;
        bitmapPrecisionBudget(source,source.width,source.height);
        const words=new preciseBitmapWords(canvasPrivateCount(source.pixels16)),stride=source.width*4;
        for(let row=0;row<source.height;row++) canvasPrivateTypedSet(words,
            canvasPrivateView(source.pixels16,row*stride,(row+1)*stride),(source.height-row-1)*stride);
        return words;
    };
    const premultiplyBitmapWords = source => {
        if (!source.pixels16) return null;
        bitmapPrecisionBudget(source,source.width,source.height);
        const words=new preciseBitmapWords(source.pixels16);
        for(let offset=0;offset<canvasPrivateCount(words);offset+=4) for(let channel=0;channel<3;channel++)
            words[offset+channel]=canvasPrivateMath.round(words[offset+channel]*words[offset+3]/65535);
        return words;
    };
    const resizeBitmapSamples = (source,width,height,smooth) => {
        if (!source.pixels16) return resampleCanvasBitmap(source,width,height,smooth);
        bitmapPrecisionBudget(source,width,height);
        // Reuse the established alpha-aware sampler. Its weighted straight/
        // associated calculation is independent of the normalized channel range.
        const result=resampleCanvasBitmap({...source,pixels:source.pixels16},width,height,smooth,true);
        return {width,height,pixels:narrowBitmapWords(result.pixels),pixels16:result.pixels};
    };
