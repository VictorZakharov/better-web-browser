    // WebGL1 pixel-store transforms also apply to WebGL2's two-dimensional
    // client views. Pixel-buffer and 3D uploads have different rejection rules.
    // Select the author subrectangle first, then reverse its rows; never flip
    // skipped rows or mutate an application-owned/possibly shared backing store.
    const webGl2ViewPixelComponents = new Map([
        [0x1906,1],[0x1909,1],[0x190a,2],[0x1903,1],[0x8227,2],
        [0x1907,3],[0x1908,4],[0x8d94,1],[0x8228,2],[0x8d98,3],[0x8d99,4],
        [0x1902,1],[0x84f9,1]
    ]);
    const webGl2ViewPixelTypes = new Map([
        [0x1400,[Int8Array,1,127]],[0x1401,[Uint8Array,1,255]],
        [0x1402,[Int16Array,2,32767]],[0x1403,[Uint16Array,2,65535]],
        [0x1404,[Int32Array,4,2147483647]],[0x1405,[Uint32Array,4,4294967295]],
        [0x1406,[Float32Array,4,1]],[0x140b,[webGlBinary16Array,2,1]],
        [0x8033,[Uint16Array,2,15]],[0x8034,[Uint16Array,2,1]],
        [0x8363,[Uint16Array,2,1]],[0x8368,[Uint32Array,4,3]],
        [0x8c3b,[Uint32Array,4,1]],[0x8c3e,[Uint32Array,4,1]],
        [0x84fa,[Uint32Array,4,1]]
    ]);
    const webGl2ViewPackedTypes = new Set([0x8033,0x8034,0x8363,0x8368,0x8c3b,0x8c3e,0x84fa]);
    const webGl2TransformView2D = (context,bytes,width,height,format,type) => {
        const state=webGl2State(context);
        if (!state.unpackFlip && !state.unpackPremultiply || !width || !height) return bytes;
        const components=webGl2ViewPixelComponents.get(format), info=webGl2ViewPixelTypes.get(type);
        // Leave malformed formats/dimensions to the native closed admission table.
        if (!components || !info || width<0 || height<0 || width>4096 || height>4096) return bytes;
        const packed=webGl2ViewPackedTypes.has(type), pixelBytes=info[1]*(packed?1:components);
        const rowLength=context.getParameter(0x0cf2)||width;
        const skipRows=context.getParameter(0x0cf3), skipPixels=context.getParameter(0x0cf4);
        const alignment=context.getParameter(0x0cf5);
        if (skipPixels+width>rowLength) { webGlError(context,0x0502); return null; }
        const stride=Math.ceil(rowLength*pixelBytes/alignment)*alignment;
        const start=skipRows*stride+skipPixels*pixelBytes;
        const size=start+(height-1)*stride+width*pixelBytes;
        if (!Number.isSafeInteger(size) || size>bytes.byteLength) { webGlError(context,0x0502); return null; }
        if (size>16*1024*1024) { webGlError(context,0x0505); return null; }
        const output=new webGl2ByteArray(size);
        for (let row=0;row<height;row++) {
            const sourceRow=state.unpackFlip ? height-1-row : row;
            const begin=start+sourceRow*stride;
            Reflect.apply(webGl2ByteSet,output,[Reflect.apply(webGl2ByteSubarray,bytes,
                [begin,begin+width*pixelBytes]),start+row*stride]);
        }
        if (state.unpackPremultiply && [0x1908,0x190a,0x8d99].includes(format)) {
            const values=new info[0](output.buffer,0,Math.floor(output.byteLength/info[1]));
            for (let row=0;row<height;row++) for (let column=0;column<width;column++) {
                const index=(start+row*stride+column*pixelBytes)/info[1];
                webGl2PremultiplyViewPixel(values,index,components,type,info[2],packed);
            }
        }
        return output;
    };
    const webGl2PremultiplyViewPixel = (values,index,components,type,maximum,packed) => {
        if (packed) {
            // Only packed RGBA layouts carry alpha; RGB/depth packed words
            // preserve every bit, including NaNs in floating RGB encodings.
            const word=values[index];
            if (type===0x8033) {
                const alpha=word&15;
                values[index]=(Math.trunc(((word>>>12)&15)*alpha/15)<<12)|
                    (Math.trunc(((word>>>8)&15)*alpha/15)<<8)|
                    (Math.trunc(((word>>>4)&15)*alpha/15)<<4)|alpha;
            } else if (type===0x8034) {
                values[index]=word&1 ? word : 0;
            } else if (type===0x8368) {
                const alpha=word>>>30;
                values[index]=(Math.trunc((word&1023)*alpha/3)|
                    Math.trunc(((word>>>10)&1023)*alpha/3)<<10|
                    Math.trunc(((word>>>20)&1023)*alpha/3)<<20|alpha<<30)>>>0;
            }
            return;
        }
        const floating=type===0x1406 || type===0x140b;
        // Signed normalized formats have two representations of -1; use the
        // representable symmetric interval for the multiplication, as Chromium
        // does for signed client formats. Integer channels keep their own width.
        const signed=[0x1400,0x1402,0x1404].includes(type);
        const alpha=signed ? Math.max(-maximum,values[index+components-1]) : values[index+components-1];
        const factor=alpha/maximum;
        for (let channel=0;channel<components-1;channel++) {
            const value=signed ? Math.max(-maximum,values[index+channel]) : values[index+channel];
            values[index+channel]=floating ? value*factor : Math.trunc(value*factor);
        }
        if (signed) values[index+components-1]=alpha;
    };
