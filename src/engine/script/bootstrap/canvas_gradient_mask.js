    const canvasGradientMaskHost = __hostCall;
    const canvasGradientMaskStringify = JSON.stringify;
    const canvasGradientMaskObject = Object.create;
    const canvasGradientMaskNumbers = values => {
        const copy = canvasGradientOwnArray(values.length);
        for (let index = 0; index < values.length; index++) copy[index] = values[index];
        return copy;
    };
    const canvasPaintGradientMask = (context, state, mask, style, left, top, right, bottom) => {
        const gradient = canvasGradientGet(canvasGradientStates, style);
        if (!gradient || gradient.stops.length > 256) return false;
        const width = right - left, height = bottom - top, pixels = width * height;
        if (pixels < 256 || pixels > MAX_CANVAS_PIXELS ||
            canvasDrawingState(context).compositeOperation !== 'source-over') return false;
        // Null-prototype snapshots prevent inherited toJSON/index hooks from
        // observing or mutating private gradient state during host serialization.
        const snapshot = canvasGradientMaskObject(null);
        snapshot.width = width; snapshot.height = height; snapshot.left = left; snapshot.top = top;
        snapshot.transform = canvasGradientMaskNumbers(canvasDrawingState(context).transform);
        snapshot.kind = gradient.kind; snapshot.geometry = canvasGradientMaskNumbers(gradient.geometry);
        snapshot.stops = canvasGradientOwnArray(gradient.stops.length);
        for (let index = 0; index < gradient.stops.length; index++) {
            const stop = canvasGradientMaskObject(null), source = gradient.stops[index];
            stop.offset = source.offset; stop.channels = canvasGradientMaskNumbers(source.channels);
            snapshot.stops[index] = stop;
        }
        snapshot.opacity = canvasDrawingState(context).globalAlpha;
        const request = canvasGradientMaskStringify(snapshot);
        return paintCanvasRegion(state, left, top, right, bottom,
            region => canvasGradientMaskHost('canvasPaintGradientMask', request, region, mask,
                canvasDrawingState(context).clipBits || null, state.width, state.height, left, top));
    };
