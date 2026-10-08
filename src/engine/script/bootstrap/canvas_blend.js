    // CSS Compositing and Blending Level 1, on straight-alpha sRGB samples.
    // https://drafts.fxtf.org/compositing-1/#blending
    const canvasPorterDuff = new Set(['source-over', 'source-in', 'source-out',
        'source-atop', 'destination-over', 'destination-in', 'destination-out',
        'destination-atop', 'xor', 'copy', 'lighter']);
    const canvasSeparableBlends = new Set(['multiply', 'screen', 'overlay', 'darken',
        'lighten', 'color-dodge', 'color-burn', 'hard-light', 'soft-light',
        'difference', 'exclusion']);
    const canvasNonseparableBlends = new Set(['hue', 'saturation', 'color', 'luminosity']);
    const canvasCompositeOperators = new Set([...canvasPorterDuff,
        ...canvasSeparableBlends, ...canvasNonseparableBlends]);
    const canvasLuminosity = rgb => .3 * rgb[0] + .59 * rgb[1] + .11 * rgb[2];
    const canvasSaturation = rgb => canvasPrivateMath.max(rgb[0],rgb[1],rgb[2]) - canvasPrivateMath.min(rgb[0],rgb[1],rgb[2]);
    const canvasClipColor = rgb => {
        const lum = canvasLuminosity(rgb), low = canvasPrivateMath.min(rgb[0],rgb[1],rgb[2]), high = canvasPrivateMath.max(rgb[0],rgb[1],rgb[2]);
        if (low < 0) rgb = canvasPrivateTripletMap(rgb, value => lum + (value - lum) * lum / (lum - low));
        if (high > 1) rgb = canvasPrivateTripletMap(rgb, value => lum + (value - lum) * (1 - lum) / (high - lum));
        return rgb;
    };
    const canvasSetLuminosity = (rgb, target) => {
        const shift = target - canvasLuminosity(rgb);
        return canvasClipColor(canvasPrivateTripletMap(rgb, value => value + shift));
    };
    const canvasSetSaturation = (rgb, target) => {
        const indices = canvasPrivateSort([0, 1, 2], (a, b) => rgb[a] - rgb[b]);
        const low = indices[0], middle = indices[1], high = indices[2];
        const result = [0, 0, 0], range = rgb[high] - rgb[low];
        if (range > 0) {
            result[middle] = (rgb[middle] - rgb[low]) * target / range;
            result[high] = target;
        }
        return result;
    };
    const canvasBlendNonseparable = (mode, backdrop, source) => {
        if (mode === 'hue') return canvasSetLuminosity(
            canvasSetSaturation(source, canvasSaturation(backdrop)), canvasLuminosity(backdrop));
        if (mode === 'saturation') return canvasSetLuminosity(
            canvasSetSaturation(backdrop, canvasSaturation(source)), canvasLuminosity(backdrop));
        if (mode === 'color') return canvasSetLuminosity(source, canvasLuminosity(backdrop));
        return canvasSetLuminosity(backdrop, canvasLuminosity(source));
    };
    const canvasBlendChannel = (mode, backdrop, source) => {
        switch (mode) {
            case 'multiply': return backdrop * source;
            case 'screen': return backdrop + source - backdrop * source;
            case 'overlay': return backdrop <= 0.5 ? 2 * backdrop * source :
                1 - 2 * (1 - backdrop) * (1 - source);
            case 'darken': return canvasPrivateMath.min(backdrop, source);
            case 'lighten': return canvasPrivateMath.max(backdrop, source);
            case 'color-dodge': return backdrop === 0 ? 0 : source === 1 ? 1 :
                canvasPrivateMath.min(1, backdrop / (1 - source));
            case 'color-burn': return backdrop === 1 ? 1 : source === 0 ? 0 :
                1 - canvasPrivateMath.min(1, (1 - backdrop) / source);
            case 'hard-light': return source <= 0.5 ? 2 * backdrop * source :
                1 - 2 * (1 - backdrop) * (1 - source);
            case 'soft-light': {
                if (source <= 0.5)
                    return backdrop - (1 - 2 * source) * backdrop * (1 - backdrop);
                const d = backdrop <= 0.25 ?
                    ((16 * backdrop - 12) * backdrop + 4) * backdrop : canvasPrivateMath.sqrt(backdrop);
                return backdrop + (2 * source - 1) * (d - backdrop);
            }
            case 'difference': return canvasPrivateMath.abs(backdrop - source);
            case 'exclusion': return backdrop + source - 2 * backdrop * source;
            default: return source;
        }
    };
    const canvasPorterDuffFactors = (mode, sourceAlpha, backdropAlpha) => {
        switch (mode) {
            case 'source-in': return [backdropAlpha, 0];
            case 'source-out': return [1 - backdropAlpha, 0];
            case 'source-atop': return [backdropAlpha, 1 - sourceAlpha];
            case 'destination-over': return [1 - backdropAlpha, 1];
            case 'destination-in': return [0, sourceAlpha];
            case 'destination-out': return [0, 1 - sourceAlpha];
            case 'destination-atop': return [1 - backdropAlpha, sourceAlpha];
            case 'xor': return [1 - backdropAlpha, 1 - sourceAlpha];
            case 'copy': return [1, 0];
            default: return [1, 1 - sourceAlpha];
        }
    };
    // Source-over is the common path. Scalar straight-alpha arithmetic avoids
    // allocating source/backdrop/factor arrays for every covered pixel.
    const canvasSourceOverPixel = (pixels, offset, red, green, blue, alpha, opacity) => {
        const sourceAlpha = alpha / 255 * opacity;
        const backdropAlpha = pixels[offset + 3] / 255;
        const backdropWeight = backdropAlpha * (1 - sourceAlpha);
        const outputAlpha = sourceAlpha + backdropWeight;
        if (outputAlpha === 0) {
            pixels[offset] = pixels[offset + 1] = pixels[offset + 2] = pixels[offset + 3] = 0;
            return;
        }
        if (sourceAlpha === 0) return;
        pixels[offset] = canvasPrivateMath.round((sourceAlpha * red + backdropWeight * pixels[offset]) / outputAlpha);
        pixels[offset + 1] = canvasPrivateMath.round((sourceAlpha * green + backdropWeight * pixels[offset + 1]) / outputAlpha);
        pixels[offset + 2] = canvasPrivateMath.round((sourceAlpha * blue + backdropWeight * pixels[offset + 2]) / outputAlpha);
        pixels[offset + 3] = canvasPrivateMath.round(outputAlpha * 255);
    };
    const compositeCanvasPixelAt = (pixels, offset, source, sourceOffset, opacity, operator) => {
        if (operator === 'source-over') {
            canvasSourceOverPixel(pixels, offset, source[sourceOffset], source[sourceOffset + 1],
                source[sourceOffset + 2], source[sourceOffset + 3], opacity);
        } else compositeCanvasPixel(pixels, offset, canvasPrivateView(source, sourceOffset, sourceOffset + 4),
            opacity, operator);
    };
    const compositeCanvasPixel = (pixels, offset, color, opacity, operator) => {
        if (operator === 'source-over') {
            canvasSourceOverPixel(pixels, offset, color[0], color[1], color[2], color[3], opacity);
            return;
        }
        const sourceAlpha = color[3] / 255 * opacity;
        const backdropAlpha = pixels[offset + 3] / 255;
        const source = [color[0] / 255, color[1] / 255, color[2] / 255];
        const backdrop = [pixels[offset] / 255, pixels[offset + 1] / 255,
            pixels[offset + 2] / 255];
        const blend = canvasPrivateSetHas(canvasSeparableBlends, operator) || canvasPrivateSetHas(canvasNonseparableBlends, operator);
        const blended = canvasPrivateSetHas(canvasNonseparableBlends, operator) ?
            canvasBlendNonseparable(operator, backdrop, source) : null;
        const factors = blend ? [1, 1 - sourceAlpha] :
            canvasPorterDuffFactors(operator, sourceAlpha, backdropAlpha);
        const sourceFactor = factors[0], backdropFactor = factors[1];
        const composedAlpha = operator === 'lighter' ?
            canvasPrivateMath.min(1, sourceAlpha + backdropAlpha) :
            sourceAlpha * sourceFactor + backdropAlpha * backdropFactor;
        const outputAlpha = canvasBitmapIsOpaque(pixels) ? 1 : composedAlpha;
        for (let channel = 0; channel < 3; channel++) {
            const premultiplied = operator === 'lighter' ?
                canvasPrivateMath.min(1, sourceAlpha * source[channel] + backdropAlpha * backdrop[channel]) :
                blend ?
                    sourceAlpha * (1 - backdropAlpha) * source[channel] +
                    backdropAlpha * (1 - sourceAlpha) * backdrop[channel] +
                    sourceAlpha * backdropAlpha *
                        (blended ? blended[channel] :
                            canvasBlendChannel(operator, backdrop[channel], source[channel])) :
                    sourceAlpha * sourceFactor * source[channel] +
                    backdropAlpha * backdropFactor * backdrop[channel];
            pixels[offset + channel] = outputAlpha === 0 ? 0 :
                canvasPrivateMath.round(255 * premultiplied / outputAlpha);
        }
        pixels[offset + 3] = canvasPrivateMath.round(outputAlpha * 255);
    };
