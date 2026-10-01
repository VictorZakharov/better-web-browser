    const frameRGBA = (state, rect = state.visible) => {
        bitmapPixelBudget(rect.width, rect.height);
        const output = new Uint8ClampedArray(rect.width * rect.height * 4);
        const rgb = frameIsRGB(state.format);
        const color = videoColorState(state.color);
        if (!rgb && !['bt709', 'bt470bg', 'smpte170m', null].includes(color.matrix))
            throw frameError('This YUV color matrix is not implemented', 'NotSupportedError');
        if (!['bt709', 'bt470bg', 'smpte170m', null].includes(color.primaries) ||
            !['bt709', 'smpte170m', 'iec61966-2-1', null].includes(color.transfer))
            throw frameError('HDR and wide-gamut frame conversion is not implemented', 'NotSupportedError');
        const converted = rgb ? null : host('videoFrameRGB', state.width, state.height,
            state.format, state.planes, color.fullRange === true, color.matrix ?? 'bt709');
        for (let y = 0; y < rect.height; y++) for (let x = 0; x < rect.width; x++) {
            const sourceX = x + rect.x, sourceY = y + rect.y;
            const offset = (y * rect.width + x) * 4;
            if (rgb) {
                const source = (sourceY * state.width + sourceX) * 4;
                const bgra = state.format.startsWith('BG');
                const pixels = state.planes[0];
                output[offset] = pixels[source + (bgra ? 2 : 0)];
                output[offset + 1] = pixels[source + 1];
                output[offset + 2] = pixels[source + (bgra ? 0 : 2)];
                output[offset + 3] = state.format.endsWith('X') ? 255 : pixels[source + 3];
            } else {
                const source = (sourceY * state.width + sourceX) * 4;
                output.set(converted.subarray(source, source + 4), offset);
            }
        }
        return output;
    };
    const videoFrameSnapshot = frame => {
        const state = activeVideoFrame(frame);
        const input = frameRGBA(state);
        const swapped = state.rotation === 90 || state.rotation === 270;
        const width = swapped ? state.visible.height : state.visible.width;
        const height = swapped ? state.visible.width : state.visible.height;
        const pixels = new Uint8ClampedArray(input.length);
        for (let y = 0; y < state.visible.height; y++) for (let x = 0; x < state.visible.width; x++) {
            let targetX = x, targetY = y;
            if (state.rotation === 90) { targetX = state.visible.height - 1 - y; targetY = x; }
            else if (state.rotation === 180) { targetX = width - 1 - x; targetY = height - 1 - y; }
            else if (state.rotation === 270) { targetX = y; targetY = state.visible.width - 1 - x; }
            if (state.flip) targetX = width - 1 - targetX;
            const start = (y * state.visible.width + x) * 4;
            pixels.set(input.subarray(start, start + 4), (targetY * width + targetX) * 4);
        }
        const result = {width, height, pixels};
        if (state.displayWidth === width && state.displayHeight === height) return result;
        return formatImageBitmap(result, {resizeWidth: state.displayWidth,
            resizeHeight: state.displayHeight, resizeQuality: 'low',
            imageOrientation: 'from-image', premultiplyAlpha: 'none'});
    };
