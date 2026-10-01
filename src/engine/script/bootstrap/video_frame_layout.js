    // Canonical tightly packed planes keep original samples for lossless copyTo.
    // Only conversion to an RGB Canvas source performs YUV matrix arithmetic.
    const frameFormatInfo = format => {
        switch (format) {
        case 'RGBA': case 'RGBX': case 'BGRA': case 'BGRX':
            return [{x: 1, y: 1, bytes: 4}];
        case 'I420': return [{x:1,y:1,bytes:1}, {x:2,y:2,bytes:1}, {x:2,y:2,bytes:1}];
        case 'I420A': return [{x:1,y:1,bytes:1}, {x:2,y:2,bytes:1},
            {x:2,y:2,bytes:1}, {x:1,y:1,bytes:1}];
        case 'I422': return [{x:1,y:1,bytes:1}, {x:2,y:1,bytes:1}, {x:2,y:1,bytes:1}];
        case 'I422A': return [{x:1,y:1,bytes:1}, {x:2,y:1,bytes:1},
            {x:2,y:1,bytes:1}, {x:1,y:1,bytes:1}];
        case 'I444': return [{x:1,y:1,bytes:1}, {x:1,y:1,bytes:1}, {x:1,y:1,bytes:1}];
        case 'I444A': return [{x:1,y:1,bytes:1}, {x:1,y:1,bytes:1},
            {x:1,y:1,bytes:1}, {x:1,y:1,bytes:1}];
        case 'NV12': return [{x:1,y:1,bytes:1}, {x:2,y:2,bytes:2}];
        default: throw frameError('This frame sample format is not implemented', 'NotSupportedError');
        }
    };
    const frameIsRGB = format => ['RGBA', 'RGBX', 'BGRA', 'BGRX'].includes(format);
    const frameAligned = (format, rect) => {
        for (const plane of frameFormatInfo(format)) {
            if (rect.x % plane.x || rect.y % plane.y)
                throw new TypeError('Frame rectangle origin is not aligned to its chroma samples');
        }
    };
    const frameLayout = (format, width, height, requested) => {
        const planes = frameFormatInfo(format);
        if (requested !== undefined && requested.length !== planes.length)
            throw new TypeError('Frame layout has the wrong number of planes');
        let size = 0;
        const ranges = [];
        const layouts = planes.map((plane, index) => {
            const columns = Math.ceil(width / plane.x), rows = Math.ceil(height / plane.y);
            const rowBytes = columns * plane.bytes;
            const offset = requested?.[index].offset ?? size;
            const stride = requested?.[index].stride ?? rowBytes;
            if (stride < rowBytes) throw new TypeError('Frame stride is smaller than its row');
            // WebCodecs counts the complete final stride, even though copyTo
            // leaves padding bytes untouched. It also governs plane overlap.
            const end = offset + stride * rows;
            if (!Number.isSafeInteger(end) || end > 64 * 1024 * 1024)
                throw frameError('Frame layout exceeds the 64 MiB buffer budget', 'NotSupportedError');
            for (const [start, previousEnd] of ranges) {
                if (offset < previousEnd && end > start)
                    throw new TypeError('Frame planes overlap');
            }
            ranges.push([offset, end]);
            size = Math.max(size, end);
            return {offset, stride, rowBytes, rows, columns, ...plane};
        });
        return {layouts, size};
    };
    const frameReadPlanes = (bytes, format, width, height, layout) => {
        const computed = frameLayout(format, width, height, layout);
        if (bytes.byteLength < computed.size) throw new TypeError('Frame buffer is too small for its layout');
        return computed.layouts.map(plane => {
            const output = new Uint8Array(plane.rowBytes * plane.rows);
            for (let row = 0; row < plane.rows; row++) {
                const start = plane.offset + row * plane.stride;
                output.set(bytes.subarray(start, start + plane.rowBytes), row * plane.rowBytes);
            }
            return output;
        });
    };
    const frameCopyDictionary = value => {
        const options = frameDictionary(value);
        const colorSpace = frameEnum(options.colorSpace, ['srgb', 'display-p3'], 'srgb');
        const rawFormat = options.format;
        const format = frameEnum(rawFormat, frameFormats, undefined);
        const layout = frameLayoutOptions(options.layout);
        const rectangle = frameRectOptions(options.rect);
        return {colorSpace, format, layout, rectangle};
    };
    const frameCopyOptions = (options, state) => {
        const {colorSpace, layout, rectangle} = options;
        const format = options.format ?? state.format;
        const rect = frameRect(rectangle ?? state.visible, state.width, state.height);
        frameAligned(state.format, rect);
        frameAligned(format, {x:0,y:0,width:rect.width,height:rect.height});
        if (options.format !== undefined && !frameIsRGB(format))
            throw frameError('Conversion to YUV output is not implemented', 'NotSupportedError');
        if (frameIsRGB(format) && colorSpace !== 'srgb')
            throw frameError('Display-P3 frame conversion is not implemented', 'NotSupportedError');
        return {format, rect, ...frameLayout(format, rect.width, rect.height, layout)};
    };
    const frameCopyPlanes = (state, bytes, options) => {
        if (bytes.length < options.size) throw new TypeError('Frame destination is smaller than allocationSize');
        const {format, rect, layouts} = options;
        if (format !== state.format) {
            const rgba = frameRGBA(state, rect), target = layouts[0];
            const bgra = format.startsWith('BG');
            const opaque = format.endsWith('X');
            for (let y = 0; y < rect.height; y++) for (let x = 0; x < rect.width; x++) {
                const source = (y * rect.width + x) * 4;
                const offset = target.offset + y * target.stride + x * 4;
                bytes[offset] = rgba[source + (bgra ? 2 : 0)];
                bytes[offset + 1] = rgba[source + 1];
                bytes[offset + 2] = rgba[source + (bgra ? 0 : 2)];
                bytes[offset + 3] = opaque ? 255 : rgba[source + 3];
            }
        } else {
            const info = frameFormatInfo(format);
            for (let index = 0; index < layouts.length; index++) {
                const plane = info[index], target = layouts[index];
                const stride = Math.ceil(state.width / plane.x) * plane.bytes;
                const origin = rect.y / plane.y * stride + rect.x / plane.x * plane.bytes;
                for (let row = 0; row < target.rows; row++) {
                    const start = origin + row * stride;
                    bytes.set(state.planes[index].subarray(start, start + target.rowBytes),
                        target.offset + row * target.stride);
                }
            }
        }
        return layouts.map(plane => ({offset: plane.offset, stride: plane.stride}));
    };
