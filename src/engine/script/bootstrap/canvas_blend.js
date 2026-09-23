    // CSS Compositing and Blending 1, applied to straight-alpha sRGB samples.
    // https://drafts.fxtf.org/compositing-1/#blending
    const canvasCompositeOperators = new Set([
        'source-over', 'destination-over', 'copy', 'lighter', 'multiply', 'screen'
    ]);
    const compositeCanvasPixel = (pixels, offset, color, opacity, operator) => {
        const sourceAlpha = color[3] / 255 * opacity;
        const destinationAlpha = pixels[offset + 3] / 255;
        if (operator === 'copy') {
            for (let channel = 0; channel < 3; channel++) pixels[offset + channel] = color[channel];
            pixels[offset + 3] = Math.round(sourceAlpha * 255);
            return;
        }
        let outputAlpha;
        if (operator === 'lighter') outputAlpha = Math.min(1, sourceAlpha + destinationAlpha);
        else outputAlpha = sourceAlpha + destinationAlpha * (1 - sourceAlpha);
        if (operator === 'destination-over')
            outputAlpha = destinationAlpha + sourceAlpha * (1 - destinationAlpha);
        for (let channel = 0; channel < 3; channel++) {
            const source = color[channel] / 255;
            const backdrop = pixels[offset + channel] / 255;
            let premultiplied;
            switch (operator) {
                case 'destination-over':
                    premultiplied = destinationAlpha * backdrop + sourceAlpha * (1 - destinationAlpha) * source;
                    break;
                case 'lighter':
                    premultiplied = Math.min(1, sourceAlpha * source + destinationAlpha * backdrop);
                    break;
                default: {
                    const blend = operator === 'screen' ? backdrop + source - backdrop * source :
                        operator === 'multiply' ? backdrop * source : source;
                    premultiplied = (1 - destinationAlpha) * sourceAlpha * source +
                        (1 - sourceAlpha) * destinationAlpha * backdrop +
                        sourceAlpha * destinationAlpha * blend;
                    break;
                }
            }
            pixels[offset + channel] = outputAlpha === 0 ? 0 :
                Math.round(255 * premultiplied / outputAlpha);
        }
        pixels[offset + 3] = Math.round(outputAlpha * 255);
    };
