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
    const canvasSaturation = rgb => Math.max(...rgb) - Math.min(...rgb);
    const canvasClipColor = rgb => {
        const lum = canvasLuminosity(rgb), low = Math.min(...rgb);
        if (low < 0) rgb = rgb.map(value => lum + (value - lum) * lum / (lum - low));
        const high = Math.max(...rgb);
        if (high > 1) rgb = rgb.map(value => lum + (value - lum) * (1 - lum) / (high - lum));
        return rgb;
    };
    const canvasSetLuminosity = (rgb, target) => {
        const shift = target - canvasLuminosity(rgb);
        return canvasClipColor(rgb.map(value => value + shift));
    };
    const canvasSetSaturation = (rgb, target) => {
        const indices = [0, 1, 2].sort((a, b) => rgb[a] - rgb[b]);
        const [low, middle, high] = indices;
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
            case 'darken': return Math.min(backdrop, source);
            case 'lighten': return Math.max(backdrop, source);
            case 'color-dodge': return source === 1 ? 1 :
                Math.min(1, backdrop / (1 - source));
            case 'color-burn': return source === 0 ? 0 :
                1 - Math.min(1, (1 - backdrop) / source);
            case 'hard-light': return source <= 0.5 ? 2 * backdrop * source :
                1 - 2 * (1 - backdrop) * (1 - source);
            case 'soft-light': {
                if (source <= 0.5)
                    return backdrop - (1 - 2 * source) * backdrop * (1 - backdrop);
                const d = backdrop <= 0.25 ?
                    ((16 * backdrop - 12) * backdrop + 4) * backdrop : Math.sqrt(backdrop);
                return backdrop + (2 * source - 1) * (d - backdrop);
            }
            case 'difference': return Math.abs(backdrop - source);
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
    const compositeCanvasPixel = (pixels, offset, color, opacity, operator) => {
        const sourceAlpha = color[3] / 255 * opacity;
        const backdropAlpha = pixels[offset + 3] / 255;
        const source = [color[0] / 255, color[1] / 255, color[2] / 255];
        const backdrop = [pixels[offset] / 255, pixels[offset + 1] / 255,
            pixels[offset + 2] / 255];
        const blend = canvasSeparableBlends.has(operator) || canvasNonseparableBlends.has(operator);
        const blended = canvasNonseparableBlends.has(operator) ?
            canvasBlendNonseparable(operator, backdrop, source) : null;
        const [sourceFactor, backdropFactor] = blend ? [1, 1 - sourceAlpha] :
            canvasPorterDuffFactors(operator, sourceAlpha, backdropAlpha);
        const outputAlpha = operator === 'lighter' ?
            Math.min(1, sourceAlpha + backdropAlpha) :
            sourceAlpha * sourceFactor + backdropAlpha * backdropFactor;
        for (let channel = 0; channel < 3; channel++) {
            const premultiplied = operator === 'lighter' ?
                Math.min(1, sourceAlpha * source[channel] + backdropAlpha * backdrop[channel]) :
                blend ?
                    sourceAlpha * (1 - backdropAlpha) * source[channel] +
                    backdropAlpha * (1 - sourceAlpha) * backdrop[channel] +
                    sourceAlpha * backdropAlpha *
                        (blended ? blended[channel] :
                            canvasBlendChannel(operator, backdrop[channel], source[channel])) :
                    sourceAlpha * sourceFactor * source[channel] +
                    backdropAlpha * backdropFactor * backdrop[channel];
            pixels[offset + channel] = outputAlpha === 0 ? 0 :
                Math.round(255 * premultiplied / outputAlpha);
        }
        pixels[offset + 3] = Math.round(outputAlpha * 255);
    };
