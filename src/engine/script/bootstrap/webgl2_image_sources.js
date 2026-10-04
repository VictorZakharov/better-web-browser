    const webGl2IsImage = value => imageBitmapStates.has(value) || videoFrameStates.has(value) ||
        imageDataStates.has(value) || offscreenCanvasBrands.has(value) ||
        typeof nodeHandles !== 'undefined' && nodeHandles.has(value) &&
        (value instanceof HTMLCanvasElement ||
            typeof HTMLImageElement !== 'undefined' && value instanceof HTMLImageElement);
    const webGl2ImageArgument = value => {
        if (!webGl2IsImage(value)) throw new TypeError('Expected a TexImageSource');
        return value;
    };
    const webGl2TextureArgument = value => {
        if (webGl2IsImage(value)) return {image:value};
        return webGl2PixelArgument(value);
    };
    const webGl2ImagePixels = (context,source,width,height,depth,format,type,volume) => {
        const snapshot=imageSourceSnapshot(source,true), state=webGl2State(context);
        const bitmap=imageBitmapStates.get(source);
        if (bitmap) snapshot.pixels=bitmap.pixels;
        const components=new Map([[0x1903,1],[0x1906,1],[0x1909,1],[0x190a,2],
            [0x8227,2],[0x1907,3],[0x1908,4],[0x8d94,1],[0x8228,2],[0x8d98,3],[0x8d99,4]]).get(format);
        const integer=[0x8d94,0x8228,0x8d98,0x8d99].includes(format);
        const constructors=new Map([[0x1401,Uint8Array],[0x1406,Float32Array],[0x140b,webGlBinary16Array]]);
        const packing=webGl2ImagePacking(format,type);
        const constructor=packing?.constructor ?? constructors.get(type);
        if (!components || !constructor || integer && type!==0x1401) { webGlError(context,0x0502); return null; }
        if (width===undefined) width=snapshot.width;
        if (height===undefined) height=snapshot.height;
        const skipPixels=context.getParameter(0x0cf4), skipRows=context.getParameter(0x0cf3);
        const imageHeight=volume ? context.getParameter(0x806e)||height : height;
        const skipImages=volume ? context.getParameter(0x806d) : 0;
        if (width<0 || height<0 || depth<0) { webGlError(context,0x0501); return null; }
        const lastRow=skipRows+skipImages*imageHeight+(Math.max(1,depth)-1)*imageHeight+height;
        if (skipPixels+width>snapshot.width || lastRow>snapshot.height ||
            volume && imageHeight<height) { webGlError(context,0x0502); return null; }
        const count=width*height*depth*(packing?1:components);
        const size=count*(packing?.bytes ?? (type===0x1401?1:type===0x140b?2:4));
        if (!Number.isSafeInteger(size) || size>16*1024*1024) { webGlError(context,0x0505); return null; }
        const values=new constructor(count);
        for (let layer=0;layer<depth;layer++) for (let row=0;row<height;row++) for (let column=0;column<width;column++) {
            const selectedY=skipRows+(skipImages+layer)*imageHeight+row;
            const y=!bitmap && state.unpackFlip ? snapshot.height-1-selectedY : selectedY;
            const input=(y*snapshot.width+skipPixels+column)*4;
            const output=((layer*height+row)*width+column)*(packing?1:components);
            const alpha=snapshot.pixels[input+3];
            const channel=index => {
                const value=snapshot.pixels[input+index]*(!bitmap && state.unpackPremultiply && index!==3 ? alpha/255 : 1);
                // Packed DOM conversion uses an eight-bit intermediate, like
                // the decoded image pipeline. Preserve full precision for the
                // direct FLOAT/HALF_FLOAT routes instead.
                return packing ? Math.round(value) :
                    type===0x1401 ? Math.round(value) : value/255;
            };
            if (packing) { values[output]=packing.pack(channel); continue; }
            if (format===0x1906) values[output]=channel(3);
            else {
                for (let component=0;component<components;component++)
                    values[output+component]=channel(format===0x190a && component===1 ? 3 : component);
            }
        }
        return {bytes:new webGl2ByteArray(values.buffer),width,height};
    };
