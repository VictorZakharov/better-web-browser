    // Compressed offset/length arguments count source elements, including for
    // multibyte views. PBO overloads instead carry an explicit byte image size.
    // WebGL2 §3.7.6, compressedTexImage/SubImage2D/3D.
    for (const [name,prefix] of [
        ['compressedTexImage2D','uiuiii'], ['compressedTexSubImage2D','uiiiiiu'],
        ['compressedTexImage3D','uiuiiii'], ['compressedTexSubImage3D','uiiiiiiiu']]) {
        const index = prefix.length;
        const method = function(...args) {
            const state = webGl2State(this);
            if (args.length < index+1) throw new TypeError(name+' requires at least '+(index+1)+' arguments');
            const viewOverload = webGl2IsView(args[index]);
            if (!viewOverload && args.length < index+2) throw new TypeError(name+' requires a buffer offset');
            for (let slot=0;slot<index;slot++) args[slot] = webGlScalar(prefix[slot],args[slot]);
            let source,offset,length;
            if (viewOverload) {
                source = webGl2View(args[index]);
                offset = webGl2UnsignedLongLong(args[index+1]);
                length = webGlScalar('u',args[index+2]);
            } else {
                args[index] = webGlScalar('i',args[index]);
                args[index+1] = webGlLongLong(args[index+1]);
            }
            if (state.lost) return;
            if (!viewOverload) { webGl2Invoke(this,name+'FromBuffer',args.slice(0,index+2)); return; }
            const bytes = webGl2Slice(this,source,offset,length);
            if (bytes) webGl2Invoke(this,name,args.slice(0,index),[], '',bytes);
        };
        Object.defineProperties(method,{name:{value:name},length:{value:index+1}});
        Object.defineProperty(webGl2Prototype,name,{configurable:true,writable:true,value:method});
    }
