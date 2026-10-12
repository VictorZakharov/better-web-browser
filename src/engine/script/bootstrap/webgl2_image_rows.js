    // Private/snapshotted RGBA8 TexImageSource rows need no generic per-channel
    // conversion. Intrinsic views/set bypass author constructors, species and
    // getters; native admission still takes owned bytes at this public call.
    const webGl2Rgba8Rows = (snapshot,width,height,depth,skipPixels,skipRows,
        imageHeight,skipImages,flip,premultiply = false,copy = false) => {
        const pixels=snapshot.pixels;
        const name=webGl2Apply(webGl2TypedName,pixels,[]);
        if ((name!=='Uint8Array' && name!=='Uint8ClampedArray') ||
            canvasPrivateCount(pixels)!==snapshot.width*snapshot.height*4) return null;
        const buffer=canvasPrivateBuffer(pixels),base=canvasPrivateOffset(pixels);
        const stride=width*4,count=stride*height*depth;
        const firstRow=skipRows+skipImages*imageHeight;
        if (!copy && !premultiply && !flip && width===snapshot.width && skipPixels===0 &&
            (depth===1 || imageHeight===height)) {
            return new webGl2ByteArray(buffer,base+firstRow*width*4,count);
        }
        const result=new webGl2ByteArray(count);
        for(let layer=0;layer<depth;layer++) for(let row=0;row<height;row++) {
            const selected=firstRow+layer*imageHeight+row;
            const y=flip?snapshot.height-1-selected:selected;
            const input=base+(y*snapshot.width+skipPixels)*4;
            const source=new webGl2ByteArray(buffer,input,stride);
            canvasPrivateTypedSet(result,source,(layer*height+row)*stride);
        }
        if (premultiply) for(let i=0;i<count;i+=4) {
            const alpha=result[i+3];
            // The product is an integer <=65025; /255 has no half-integer
            // ties. This is the existing round(channel * alpha/255) result.
            result[i]=canvasPrivateMath.floor((result[i]*alpha+127)/255);
            result[i+1]=canvasPrivateMath.floor((result[i+1]*alpha+127)/255);
            result[i+2]=canvasPrivateMath.floor((result[i+2]*alpha+127)/255);
        }
        return result;
    };
