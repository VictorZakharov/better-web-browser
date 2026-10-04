    // WebGL2 typed uploads/readbacks are selected by intrinsic view kind, not
    // instanceof (which would reject same-origin views from another realm).
    const webGl2PixelKinds = new Map([
        [0x1400,['Int8Array']], [0x1401,['Uint8Array','Uint8ClampedArray']],
        [0x1402,['Int16Array']], [0x1403,['Uint16Array']],
        [0x1404,['Int32Array']], [0x1405,['Uint32Array']],
        [0x1406,['Float32Array']], [0x140b,['Uint16Array']],
        [0x8033,['Uint16Array']], [0x8034,['Uint16Array']], [0x8363,['Uint16Array']],
        [0x8368,['Uint32Array']], [0x8c3b,['Uint32Array']],
        [0x8c3e,['Uint32Array']], [0x84fa,['Uint32Array']]
    ]);
    const webGl2PixelArgument = value => {
        if (value === null || value === undefined) return null;
        if (webGl2IsView(value)) return webGl2View(value);
        return {offset:webGlLongLong(value)};
    };
    const webGl2PixelBytes = (context,source,type,offset=0) => {
        if (!webGl2PixelKinds.get(type)?.includes(source.name)) {
            webGlError(context,0x0502); return null;
        }
        return webGl2Slice(context,source,offset,0,0x0502);
    };
    webGl2Method('readPixels',7,count => count >= 8 ? 'iiiiuu-a' : 'iiiiuu-',
        count => [[6,count >= 8 ? webGl2View : webGl2PixelArgument]],
        function(x,y,width,height,format,type,destination,offset=0) {
            if (destination === null) { webGlError(this,0x0501); return; }
            if ('offset' in destination) {
                webGl2Invoke(this,'readPixelsToBuffer',[x,y,width,height,format,type,destination.offset]);
                return;
            }
            const bytes = webGl2PixelBytes(this,destination,type,offset);
            if (!bytes) return;
            const result = webGlCall(this,'readPixels',[x,y,width,height,format,type,bytes.byteLength],[], '',bytes);
            if (result) Reflect.apply(webGl2ByteSet,bytes,[result]);
        });
    for (const [name,signature] of [['texStorage2D','uiuii'],['texStorage3D','uiuiii']]) {
        webGl2Method(name,signature.length,signature,[],function(...args) {
            webGl2Invoke(this,name,args.slice(0,signature.length));
        });
    }
    for (const sub of [false,true]) {
        const name = sub ? 'texSubImage3D' : 'texImage3D';
        const index = sub ? 10 : 9;
        const prefix = sub ? 'uiiiiiiiuu' : 'uiiiiiiuu';
        webGl2Method(name,index+1,count => prefix+'-'+(count > index+1 ? 'a' : ''),
            count => [[index,count > index+1 ? value => sub && value == null ? null : webGl2View(value) : webGl2TextureArgument]],function(...args) {
                const source = args[index], type = args[index-1];
                const values = args.slice(0,index);
                if (source && 'image' in source) {
                    const first=sub?5:3;
                    const image=webGl2ImagePixels(this,source.image,values[first],values[first+1],values[first+2],values[index-2],type,true);
                    if (image) webGl2Invoke(this,name+'FromImage',values,[], '',image.bytes);
                    return;
                }
                const state = webGl2State(this);
                if (source !== null && (state.unpackFlip || state.unpackPremultiply)) {
                    webGlError(this,0x0502); return;
                }
                if (source && 'offset' in source) {
                    webGl2Invoke(this,name+'FromBuffer',[...values,source.offset]);
                    return;
                }
                const bytes = source === null ? undefined : webGl2PixelBytes(this,source,type,args[index+1]??0);
                if (source === null || bytes) webGl2Invoke(this,name,values,[], '',bytes);
            });
    }
