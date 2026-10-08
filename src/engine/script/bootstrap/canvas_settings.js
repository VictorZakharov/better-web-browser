    // Canvas settings belong to the context, not the save/restore drawing state.
    // Only sRGB/unorm8 has a real paint backend; other valid formats fail creation
    // instead of exposing a claimed color space with an RGBA8/sRGB bitmap.
    const canvasContextSettings = new WeakMap();
    const canvasOpaqueBitmaps = new WeakSet();
    const canvasSettingsGet = Function.call.bind(WeakMap.prototype.get);
    const canvasSettingsSet = Function.call.bind(WeakMap.prototype.set);
    const canvasOpaqueHas = Function.call.bind(WeakSet.prototype.has);
    const canvasOpaqueAdd = Function.call.bind(WeakSet.prototype.add);
    const canvasSettingsFill = Function.call.bind(Uint8ClampedArray.prototype.fill);
    const canvasSettingsEnum = (value, fallback, allowed, name) => {
        if (value === undefined) return fallback;
        const converted = `${value}`;
        if (!allowed.includes(converted)) throw new TypeError(`Invalid Canvas ${name}`);
        return converted;
    };
    const canvasConvertSettings = options => {
        if (options !== undefined && options !== null &&
            typeof options !== 'object' && typeof options !== 'function')
            throw new TypeError('Canvas settings must be a dictionary');
        // Web IDL dictionary members are read and converted in lexical order.
        const rawAlpha = options?.alpha;
        const alpha = rawAlpha === undefined ? true : !!rawAlpha;
        const colorSpace = canvasSettingsEnum(options?.colorSpace, 'srgb',
            ['srgb','display-p3'], 'colorSpace');
        const colorType = canvasSettingsEnum(options?.colorType, 'unorm8',
            ['unorm8','float16'], 'colorType');
        const desynchronized = !!options?.desynchronized;
        const willReadFrequently = !!options?.willReadFrequently;
        return {alpha, colorSpace, colorType, desynchronized, willReadFrequently};
    };
    const canvasSettingsSupported = settings =>
        settings.colorSpace === 'srgb' && settings.colorType === 'unorm8';
    const canvasBitmapIsOpaque = pixels => !!pixels && canvasOpaqueHas(canvasOpaqueBitmaps, pixels);
    const canvasClearBitmapRange = (pixels, start, end) => {
        canvasSettingsFill(pixels, 0, start, end);
        if (canvasBitmapIsOpaque(pixels))
            for (let offset = start + 3; offset < end; offset += 4) pixels[offset] = 255;
    };
    const canvasInitializeOutputBitmap = (context, pixels) => {
        const settings = canvasSettingsGet(canvasContextSettings, context);
        if (pixels && settings && !settings.alpha) {
            canvasOpaqueAdd(canvasOpaqueBitmaps, pixels);
            canvasClearBitmapRange(pixels, 0, canvasPrivateCount(pixels));
        }
    };
    const canvasInitializeContextSettings = (context, settings) => {
        // Software readback remains synchronous. No front-buffer rendering is
        // implemented, so report desynchronized:false even when it was requested.
        canvasSettingsSet(canvasContextSettings, context, {
            alpha:settings.alpha, colorSpace:'srgb', colorType:'unorm8',
            desynchronized:false, willReadFrequently:settings.willReadFrequently
        });
        canvasInitializeOutputBitmap(context, stateForCanvas(canvasPrivateWeakGet(canvas2dOwners, context)).pixels);
    };
    const canvasGetContextSettings = context => {
        const settings = canvasSettingsGet(canvasContextSettings, context);
        return {alpha:settings.alpha, colorSpace:settings.colorSpace, colorType:settings.colorType,
            desynchronized:settings.desynchronized, willReadFrequently:settings.willReadFrequently};
    };
