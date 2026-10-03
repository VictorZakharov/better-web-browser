    const webGlBinary16Array = Float16Array;
    webGlMethod('pixelStorei', function(pname, parameter) {
        pname = webGlUnsigned(pname);
        const state = webGlState(this);
        if (pname === 0x9240) { state.unpackFlip = Boolean(parameter); return; }
        if (pname === 0x9241) { state.unpackPremultiply = Boolean(parameter); return; }
        if (pname === 0x9243) {
            const value = webGlUnsigned(parameter);
            if (![0, 0x9244].includes(value)) { webGlError(this, 0x0501); return; }
            state.unpackColorSpace = value; return;
        }
        webGlCall(this, 'pixelStorei', [pname, webGlInteger(parameter)]);
    });
    const webGlImagePixels = (context, source, format, type) => {
        // Canvas imageSourceSnapshot admits only decoded same-origin/CORS-readable images.
        // WebGL never obtains opaque network pixels through a separate native decoding path.
        const snapshot = imageSourceSnapshot(source, true);
        if (![0x1401,0x1406,0x8d61].includes(type) || ![0x1908, 0x1907, 0x1906, 0x1909, 0x190a].includes(format)) {
            webGlError(context, 0x0502); return null;
        }
        const state = webGlState(context);
        const bitmap = imageBitmapStates.get(source);
        if (bitmap) snapshot.pixels = new Uint8ClampedArray(bitmap.pixels);
        const components = format === 0x1908 ? 4 : format === 0x1907 ? 3 : format === 0x190a ? 2 : 1;
        const alignment = context.getParameter(0x0cf5);
        const componentBytes = type===0x1406 ? 4 : type===0x8d61 ? 2 : 1;
        const rowBytes = snapshot.width * components * componentBytes;
        const stride = Math.ceil(rowBytes / alignment) * alignment;
        if (stride * snapshot.height > 16*1024*1024) { webGlError(context,0x0505); return null; }
        const pixels = new Uint8Array(stride * snapshot.height);
        // Reuse V8's IEEE binary16 conversion rather than a second half-float
        // implementation. The packed upload still uses WebGL's Uint16Array ABI.
        const values = type===0x1406 ? new Float32Array(pixels.buffer) :
            type===0x8d61 ? new webGlBinary16Array(pixels.buffer) : pixels;
        for (let y = 0; y < snapshot.height; y++) for (let x = 0; x < snapshot.width; x++) {
            const sourceY = !bitmap && state.unpackFlip ? snapshot.height - 1 - y : y;
            const input = (sourceY * snapshot.width + x) * 4;
            const output = y * stride/componentBytes + x * components;
            const alpha = snapshot.pixels[input + 3];
            const convert = value => componentBytes===1 ? Math.round(value) : value/255;
            const channel = offset => convert(!bitmap && state.unpackPremultiply ? snapshot.pixels[input + offset] * alpha / 255 : snapshot.pixels[input + offset]);
            if (format === 0x1906) values[output] = convert(alpha);
            else if (format === 0x1909 || format === 0x190a) {
                values[output] = channel(0);
                if (components === 2) values[output + 1] = convert(alpha);
            } else {
                for (let offset = 0; offset < 3; offset++) values[output + offset] = channel(offset);
                if (components === 4) values[output + 3] = convert(alpha);
            }
        }
        return {pixels, width:snapshot.width, height:snapshot.height};
    };
    for (const name of ['texImage2D', 'texSubImage2D']) webGlMethod(name, function(...args) {
        webGlState(this);
        const sub = name === 'texSubImage2D';
        const imageOverload = args.length === (sub ? 7 : 6);
        let target, level, width, height, format, type, pixels, internal, border, x = 0, y = 0;
        if (imageOverload) {
            if (sub) [target, level, x, y, format, type] = args;
            else [target, level, internal, format, type] = args;
            const snapshot = webGlImagePixels(this, args[args.length - 1], webGlUnsigned(format), webGlUnsigned(type));
            if (!snapshot) return;
            ({pixels, width, height} = snapshot); border = 0;
        } else {
            if (args.length !== 9) throw new TypeError(name + ' requires 6/7 or 9 arguments');
            if (sub) [target, level, x, y, width, height, format, type, pixels] = args;
            else [target, level, internal, width, height, border, format, type, pixels] = args;
            if (pixels !== null) {
                const kind=webGlUnsigned(type);
                const valid=kind===0x1401 ? pixels instanceof Uint8Array || pixels instanceof Uint8ClampedArray :
                    kind===0x1406 ? pixels instanceof Float32Array :
                    [0x1405,0x84fa].includes(kind) ? pixels instanceof Uint32Array :
                    [0x1403,0x8d61,0x8363,0x8033,0x8034].includes(kind) && pixels instanceof Uint16Array;
                if (!valid) {
                    webGlError(this, 0x0502); return;
                }
                pixels = webGlBytes(pixels);
            }
        }
        webGlCall(this, name, [webGlUnsigned(target), webGlInteger(level),
            sub ? webGlInteger(x) : webGlUnsigned(internal), webGlInteger(width), webGlInteger(height),
            sub ? webGlInteger(y) : webGlInteger(border), webGlUnsigned(format), webGlUnsigned(type)], [], '', pixels ?? undefined);
    });
