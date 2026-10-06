    // ImageData owns an actual typed-array view, not a copy of it. Platform
    // dimensions and pixels are read through private slots and captured intrinsics.
    // https://html.spec.whatwg.org/multipage/imagebitmap-and-animations.html#imagedata
    const imageDataStates = new WeakMap();
    const imageDataWeakGet = Function.call.bind(WeakMap.prototype.get);
    const imageDataWeakHas = Function.call.bind(WeakMap.prototype.has);
    const imageDataWeakSet = Function.call.bind(WeakMap.prototype.set);
    const imageDataBufferLength = Function.call.bind(
        Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, 'byteLength').get);
    const imageDataResizable = Function.call.bind(
        Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, 'resizable').get);
    const imageDataState = value => {
        const state = imageDataWeakGet(imageDataStates, value);
        if (!state) throw new TypeError('Illegal ImageData receiver');
        return state;
    };
    const imageDataOptions = options => {
        if (options !== undefined && options !== null &&
            typeof options !== 'object' && typeof options !== 'function')
            throw new TypeError('ImageData settings must be a dictionary');
        const colorSpace = canvasSettingsEnum(options?.colorSpace, 'srgb',
            ['srgb','display-p3'], 'ImageData colorSpace');
        const pixelFormat = canvasSettingsEnum(options?.pixelFormat, 'rgba-unorm8',
            ['rgba-unorm8','rgba-float16'], 'ImageData pixelFormat');
        return {colorSpace, pixelFormat};
    };
    const imageDataFormatAvailable = settings => {
        if (settings.colorSpace !== 'srgb' || settings.pixelFormat !== 'rgba-unorm8')
            throw new DOMException('Only sRGB RGBA8 ImageData has a pixel backend', 'NotSupportedError');
    };
    const imageDataUnsignedLong = value => +value >>> 0;
    const imageDataArrayKind = value => {
        try { return canvasPixelTag(value); } catch { return undefined; }
    };
    class ImageData {
        constructor(dataOrWidth, widthOrHeight, heightOrSettings, options) {
            if (arguments.length < 2) throw new TypeError('ImageData requires two arguments');
            const kind = imageDataArrayKind(dataOrWidth);
            const array = kind === 'Uint8ClampedArray' || kind === 'Float16Array';
            let width, height, settings, data;
            if (array) {
                // Buffer-source conversion rejects shared and resizable storage
                // before dimension/settings getters, as required by Web IDL.
                const buffer = canvasPixelBuffer(dataOrWidth);
                imageDataBufferLength(buffer);
                if (imageDataResizable(buffer)) throw new TypeError('Resizable ImageData storage is unsupported');
                width = imageDataUnsignedLong(widthOrHeight);
                height = heightOrSettings === undefined ? undefined : imageDataUnsignedLong(heightOrSettings);
                settings = imageDataOptions(options);
                const bytes = canvasPixelLength(dataOrWidth);
                const stride = settings.pixelFormat === 'rgba-unorm8' ? 4 : 8;
                if (!bytes || bytes % stride)
                    throw new DOMException('ImageData storage has no integral pixels', 'InvalidStateError');
                const pixels = bytes / stride;
                if (!width || pixels % width)
                    throw new DOMException('ImageData width does not match its data', 'IndexSizeError');
                const rows = pixels / width;
                if (height !== undefined && height !== rows)
                    throw new DOMException('ImageData height does not match its data', 'IndexSizeError');
                height = rows;
                if ((settings.pixelFormat === 'rgba-unorm8') !== (kind === 'Uint8ClampedArray'))
                    throw new DOMException('ImageData array type does not match pixelFormat', 'InvalidStateError');
                data = dataOrWidth;
            } else {
                width = imageDataUnsignedLong(dataOrWidth);
                height = imageDataUnsignedLong(widthOrHeight);
                settings = imageDataOptions(heightOrSettings);
                if (!width || !height)
                    throw new DOMException('ImageData dimensions must be positive', 'IndexSizeError');
            }
            imageDataFormatAvailable(settings);
            if (width * height > MAX_CANVAS_PIXELS)
                throw new DOMException('ImageData exceeds the bitmap budget', 'NotSupportedError');
            if (!data) data = new canvasPixelArray(width * height * 4);
            imageDataWeakSet(imageDataStates, this, {data, width, height,
                colorSpace:settings.colorSpace, pixelFormat:settings.pixelFormat});
        }
        get data() { return imageDataState(this).data; }
        get width() { return imageDataState(this).width; }
        get height() { return imageDataState(this).height; }
        get colorSpace() { return imageDataState(this).colorSpace; }
        get pixelFormat() { return imageDataState(this).pixelFormat; }
    }
    for (const name of ['data','width','height','colorSpace','pixelFormat']) {
        const descriptor = Object.getOwnPropertyDescriptor(ImageData.prototype, name);
        descriptor.enumerable = true;
        Object.defineProperty(ImageData.prototype, name, descriptor);
    }
    Object.defineProperty(ImageData, 'length', {value:2, configurable:true});
    const createCanvasImageData = (context, args) => {
        canvasImageDataContext(context);
        if (!args.length) throw new TypeError('createImageData requires dimensions or ImageData');
        if (args.length === 1) {
            const source = imageDataState(args[0]);
            return new ImageData(source.width, source.height,
                {colorSpace:source.colorSpace, pixelFormat:source.pixelFormat});
        }
        const width = canvasPixelInteger(args[0]), height = canvasPixelInteger(args[1]);
        const settings = imageDataOptions(args[2]);
        return new ImageData(Math.abs(width), Math.abs(height), settings);
    };
    globalThis.__cloneImageDataBindings = {
        has:value => imageDataWeakHas(imageDataStates,value),
        snapshot:value => {
            const state = imageDataState(value);
            return {data:state.data,width:state.width,height:state.height,
                colorSpace:state.colorSpace,pixelFormat:state.pixelFormat};
        },
        receive:(record, data) => new ImageData(data, record.w, record.h,
            {colorSpace:record.cs ?? 'srgb', pixelFormat:record.pf ?? 'rgba-unorm8'})
    };
