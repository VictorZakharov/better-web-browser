    // Web IDL conversion is synchronous; decode/Promise settlement is not.
    // Dictionary members are visited once in lexicographic order, including
    // getters, before taking an image snapshot or waiting for Blob decoding.
    const bitmapNumber = value => {
        if (typeof value === 'bigint') throw new TypeError('ImageBitmap numeric member cannot be a BigInt');
        return +value;
    };
    const bitmapLong = value => {
        const number = bitmapNumber(value);
        if (!Number.isFinite(number) || number === 0) return 0;
        const integer = Math.trunc(number);
        const wrapped = ((integer % 0x100000000) + 0x100000000) % 0x100000000;
        return wrapped >= 0x80000000 ? wrapped - 0x100000000 : wrapped;
    };
    const bitmapUnsignedRange = value => {
        const number = bitmapNumber(value);
        if (!Number.isFinite(number)) throw new TypeError('ImageBitmap resize dimension must be finite');
        const integer = Math.trunc(number);
        if (integer < 0 || integer > 0xffffffff)
            throw new TypeError('ImageBitmap resize dimension is out of range');
        return integer;
    };
    const bitmapEnum = (value, allowed, fallback, name) => {
        if (value === undefined) return fallback;
        if (typeof value === 'symbol') throw new TypeError('ImageBitmap enum cannot be a Symbol');
        value = String(value);
        if (!allowed.includes(value)) throw new TypeError('Invalid ImageBitmap ' + name);
        return value;
    };
    const convertImageBitmapOptions = value => {
        if (value === null || value === undefined) value = {};
        if (typeof value !== 'object' && typeof value !== 'function')
            throw new TypeError('ImageBitmap options must be a dictionary');
        const colorSpaceConversion = bitmapEnum(value.colorSpaceConversion,
            ['none', 'default'], 'default', 'colorSpaceConversion');
        // Chromium still implements "none". Preserve that compatibility value
        // as a documented legacy extension rather than conflating it with flipY.
        const imageOrientation = bitmapEnum(value.imageOrientation,
            ['from-image', 'flipY', 'none'], 'from-image', 'imageOrientation');
        const premultiplyAlpha = bitmapEnum(value.premultiplyAlpha,
            ['none', 'premultiply', 'default'], 'default', 'premultiplyAlpha');
        const rawHeight = value.resizeHeight;
        const resizeHeight = rawHeight === undefined ? undefined : bitmapUnsignedRange(rawHeight);
        const resizeQuality = bitmapEnum(value.resizeQuality,
            ['pixelated', 'low', 'medium', 'high'], 'low', 'resizeQuality');
        const rawWidth = value.resizeWidth;
        const resizeWidth = rawWidth === undefined ? undefined : bitmapUnsignedRange(rawWidth);
        return {colorSpaceConversion, imageOrientation, premultiplyAlpha,
            resizeHeight, resizeQuality, resizeWidth};
    };
    const bitmapPixelBudget = (width, height) => {
        if (!Number.isInteger(width) || !Number.isInteger(height) || width <= 0 || height <= 0 ||
            width > 0xffffffff || height > 0xffffffff || width * height > MAX_CANVAS_PIXELS)
            throw new DOMException('ImageBitmap exceeds the bitmap budget', 'NotSupportedError');
    };
    const flipBitmapVertically = source => {
        const pixels = new Uint8ClampedArray(source.pixels.length);
        const stride = source.width * 4;
        for (let row = 0; row < source.height; row++)
            pixels.set(source.pixels.subarray(row * stride, (row + 1) * stride),
                (source.height - row - 1) * stride);
        return {width: source.width, height: source.height, pixels};
    };
    const applyBitmapPremultiplication = source => {
        const pixels = new Uint8ClampedArray(source.pixels);
        for (let offset = 0; offset < pixels.length; offset += 4) {
            const alpha = pixels[offset + 3];
            for (let channel = 0; channel < 3; channel++)
                pixels[offset + channel] = Math.floor((pixels[offset + channel] * alpha + 127) / 255);
        }
        return {...source, pixels, premultiplied: true};
    };
    const bitmapStraightPixels = state => {
        const pixels = new Uint8ClampedArray(state.pixels);
        if (state.premultiplied) for (let offset = 0; offset < pixels.length; offset += 4) {
            const alpha = pixels[offset + 3];
            for (let channel = 0; channel < 3; channel++)
                pixels[offset + channel] = alpha === 0 ? 0 :
                    Math.min(255, Math.floor((pixels[offset + channel] * 255 + alpha / 2) / alpha));
        }
        return pixels;
    };
    const resizeImageBitmap = (source, width, height, quality) => {
        if (width === source.width && height === source.height) return source;
        if (quality !== 'pixelated') return resampleCanvasBitmap(source, width, height, true);
        // HTML's pixelated filter: nearest-neighbor to the closest positive
        // integer multiple, then bilinear to the exact requested dimensions.
        const intermediateWidth = source.width * Math.max(1, Math.round(width / source.width));
        const intermediateHeight = source.height * Math.max(1, Math.round(height / source.height));
        bitmapPixelBudget(intermediateWidth, intermediateHeight);
        const intermediate = resampleCanvasBitmap(source, intermediateWidth, intermediateHeight, false);
        return width === intermediateWidth && height === intermediateHeight ? intermediate :
            resampleCanvasBitmap(intermediate, width, height, true);
    };
    const formatImageBitmap = (source, options) => {
        const width = options.resizeWidth ?? (options.resizeHeight === undefined ? source.width :
            Math.ceil(source.width * options.resizeHeight / source.height));
        const height = options.resizeHeight ?? (options.resizeWidth === undefined ? source.height :
            Math.ceil(source.height * options.resizeWidth / source.width));
        bitmapPixelBudget(width, height);
        let output = resizeImageBitmap(source, width, height, options.resizeQuality);
        if (options.imageOrientation === 'flipY') output = flipBitmapVertically(output);
        if (options.premultiplyAlpha === 'premultiply') output = applyBitmapPremultiplication(output);
        return output;
    };
