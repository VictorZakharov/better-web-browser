    // Filter input is an infinite transparent image. Cropping the source at the
    // canvas edge first loses off-canvas ink that a blur or offset brings back.
    // Use a bounded halo and crop only after filters and the separate shadow.
    const canvasFilterHalo = settings => {
        let halo = 0;
        for (let index = 0; index < settings.filterOperations.length; index++) {
            const {name, value} = settings.filterOperations[index];
            if (name === 'blur' && value >= 0.01) halo += canvasPrivateMath.ceil(value * 4) + 2;
            if (name === 'drop-shadow') halo += canvasPrivateMath.ceil(canvasPrivateMath.max(canvasPrivateMath.abs(value.x),
                canvasPrivateMath.abs(value.y))) + (value.blur >= 0.01 ? canvasPrivateMath.ceil(value.blur * 4) + 2 : 0);
        }
        if (halo && settings.shadowColor.channels[3]) halo +=
            canvasPrivateMath.ceil(canvasPrivateMath.max(canvasPrivateMath.abs(settings.shadowOffsetX), canvasPrivateMath.abs(settings.shadowOffsetY))) +
            canvasPrivateMath.ceil(settings.shadowBlur * 2) + 2;
        return halo;
    };
    const canvasFilterCrop = (pixels, width, height, halo, originalWidth, originalHeight) => {
        if (!halo) return pixels;
        const result = new canvasPrivatePixelArray(originalWidth * originalHeight * 4);
        for (let y = 0; y < originalHeight; y++) {
            const start = ((y + halo) * width + halo) * 4;
            copyCanvasPixelRow(result, y * originalWidth * 4, pixels, start, originalWidth * 4);
        }
        return result;
    };
