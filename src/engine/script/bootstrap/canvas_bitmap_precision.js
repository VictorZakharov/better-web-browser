    // Private normalized source words. Canvas presentation stays RGBA8; only
    // genuinely higher-precision decoded sources receive this owned sidecar.
    // HTML ImageBitmap copies/transfers bitmap data, including source precision.
    // https://html.spec.whatwg.org/multipage/imagebitmap-and-animations.html#imagebitmap
    const preciseBitmapWords = Uint16Array, preciseBitmapBytes = Uint8Array;
    const preciseBitmapDataView = DataView;
    const bitmapPrecisionBudget = (source,width,height) => {
        if (!source.pixels16) return;
        bitmapPixelBudget(width,height);
        // Source + destination word/byte views, with an equal allowance for
        // snapshots and serialization. Reject before allocating, not afterwards.
        const bytes=source.pixels16.byteLength+source.pixels.byteLength+width*height*12;
        if (!Number.isSafeInteger(bytes) || bytes*2>64*1024*1024)
            throw new DOMException('Precise bitmap exceeds the working budget','NotSupportedError');
    };
    const decodedBitmapWords = bytes => {
        if (!bytes) return null;
        const words=new preciseBitmapWords(bytes.byteLength/2);
        const view=new preciseBitmapDataView(bytes.buffer,bytes.byteOffset,bytes.byteLength);
        for(let index=0;index<words.length;index++) words[index]=view.getUint16(index*2,true);
        return words;
    };
    const encodedBitmapWords = words => {
        if (!words) return null;
        const bytes=new preciseBitmapBytes(words.length*2),view=new preciseBitmapDataView(bytes.buffer);
        for(let index=0;index<words.length;index++) view.setUint16(index*2,words[index],true);
        return bytes;
    };
    const narrowBitmapWords = words => {
        const pixels=new Uint8ClampedArray(words.length);
        for(let index=0;index<words.length;index++) pixels[index]=Math.floor((words[index]+128)/257);
        return pixels;
    };
    const copyBitmapWords = state => {
        if (!state.pixels16) return null;
        bitmapPrecisionBudget(state,state.width,state.height);
        const words=new preciseBitmapWords(state.pixels16);
        if (state.premultiplied) for(let offset=0;offset<words.length;offset+=4) {
            const alpha=words[offset+3];
            for(let channel=0;channel<3;channel++) words[offset+channel]=alpha===0?0:
                Math.min(65535,Math.round(words[offset+channel]*65535/alpha));
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
            words.set(source.pixels16.subarray(input,input+4),(row*width+column)*4);
        }
        return words;
    };
    const flipBitmapWords = source => {
        if (!source.pixels16) return null;
        bitmapPrecisionBudget(source,source.width,source.height);
        const words=new preciseBitmapWords(source.pixels16.length),stride=source.width*4;
        for(let row=0;row<source.height;row++) words.set(
            source.pixels16.subarray(row*stride,(row+1)*stride),(source.height-row-1)*stride);
        return words;
    };
    const premultiplyBitmapWords = source => {
        if (!source.pixels16) return null;
        bitmapPrecisionBudget(source,source.width,source.height);
        const words=new preciseBitmapWords(source.pixels16);
        for(let offset=0;offset<words.length;offset+=4) for(let channel=0;channel<3;channel++)
            words[offset+channel]=Math.round(words[offset+channel]*words[offset+3]/65535);
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
