    // The WebGL1 image-source entry points remain shared mixin methods. WebGL2
    // adds element-offset CPU views and byte-offset pixel-buffer overloads.
    for (const sub of [false,true]) {
        const name = sub ? 'texSubImage2D' : 'texImage2D';
        const imageArity = sub ? 7 : 6;
        webGl2Method(name,imageArity,count => count < 9 ? (sub?'uiiiuu-':'uiiuu-') : 'uiiiiiuu-'+(count >= 10 ? 'a' : ''),
            count => count < 9 ? [[sub?6:5,webGl2ImageArgument]] :
                [[8,count >= 10 ? webGl2View : webGl2TextureArgument]],function(...args) {
                if (args.length < 9) {
                    const format=args[sub?4:3], type=args[sub?5:4], source=args[sub?6:5];
                    const image=webGl2ImagePixels(this,source,undefined,undefined,1,format,type,false);
                    if (!image) return;
                    const values=sub ? [args[0],args[1],args[2],image.width,image.height,args[3],format,type] :
                        [args[0],args[1],args[2],image.width,image.height,0,format,type];
                    webGl2Invoke(this,name+'FromImage',values,[], '',image.bytes); return;
                }
                const source=args[8], type=args[7];
                const values = sub ? [args[0],args[1],args[2],args[4],args[5],args[3],args[6],args[7]] : args.slice(0,8);
                if (source && 'image' in source) {
                    const image=webGl2ImagePixels(this,source.image,values[3],values[4],1,values[6],type,false);
                    if (image) webGl2Invoke(this,name+'FromImage',values,[], '',image.bytes);
                    return;
                }
                if (source && 'offset' in source) {
                    const state = webGl2State(this);
                    if (state.unpackFlip || state.unpackPremultiply) { webGlError(this,0x0502); return; }
                    webGl2Invoke(this,name+'FromBuffer',[...values,source.offset]);
                    return;
                }
                let bytes=source===null ? undefined : webGl2PixelBytes(this,source,type,args[9]??0);
                if (bytes) bytes=webGl2TransformView2D(this,bytes,values[3],values[4],values[6],type);
                if (source===null || bytes) webGl2Invoke(this,name,values,[], '',bytes);
            });
    }
