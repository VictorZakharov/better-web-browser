    // TexImageSource conversion is restricted to the WebGL2 DOM-upload table,
    // not the larger table admitted for application-owned typed pixel buffers.
    // https://registry.khronos.org/webgl/specs/latest/2.0/#DOM-uploads
    const webGl2ImagePacking = (format,type) => {
        if (type===0x8363 && format===0x1907) return {constructor:Uint16Array,bytes:2,
            pack:c => ((c(0)>>3)<<11)|((c(1)>>2)<<5)|(c(2)>>3)};
        if (type===0x8033 && format===0x1908) return {constructor:Uint16Array,bytes:2,
            pack:c => ((c(0)>>4)<<12)|((c(1)>>4)<<8)|((c(2)>>4)<<4)|(c(3)>>4)};
        if (type===0x8034 && format===0x1908) return {constructor:Uint16Array,bytes:2,
            pack:c => ((c(0)>>3)<<11)|((c(1)>>3)<<6)|((c(2)>>3)<<1)|(c(3)>>7)};
        if (type===0x8368 && format===0x1908) return {constructor:Uint32Array,bytes:4,
            pack:c => (Math.floor(c(0)*1023/255)|Math.floor(c(1)*1023/255)<<10|
                Math.floor(c(2)*1023/255)<<20|Math.floor(c(3)*3/255)<<30)>>>0};
        if (type===0x8c3b && format===0x1907) return {constructor:Uint32Array,bytes:4,
            pack:c => (webGl2ImageFloat(c(0)/255,6)|webGl2ImageFloat(c(1)/255,6)<<11|
                webGl2ImageFloat(c(2)/255,5)<<22)>>>0};
        return null;
    };
    const webGl2ImageFloat = (value,mantissaBits) => {
        // DOM channels are finite and normalized. Premultiplication can produce
        // subnormals; their exponent stays zero while the significand is rounded.
        // Both paths use round-to-nearest, ties-to-even.
        if (value===0) return 0;
        value=Math.fround(value);
        const exponent=Math.max(-14,Math.floor(Math.log2(value)));
        const normal=value>=2**-14;
        const significand=(value/2**exponent-(normal?1:0))*2**mantissaBits;
        const lower=Math.floor(significand), fraction=significand-lower;
        const rounded=lower+(fraction>0.5 || fraction===0.5 && lower%2!==0 ? 1 : 0);
        return (normal?((exponent+15)<<mantissaBits):0)+rounded;
    };
