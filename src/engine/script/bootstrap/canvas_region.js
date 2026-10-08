    // One bounded scratch region per realm, not a pixel-result cache. Every byte
    // is overwritten from the current bitmap before a native paint sees it.
    // Checking it out removes it from the pool, so nested drawing cannot alias
    // the active operation. No author ArrayBuffer is retained here.
    const MAX_CANVAS_REGION_SCRATCH_BYTES = 2 * 1024 * 1024;
    let canvasRegionScratch = null;
    const takeCanvasRegion = length => {
        const reusable = canvasRegionScratch;
        canvasRegionScratch = null;
        return reusable && canvasPixelLength(reusable) === length
            ? reusable : new canvasPixelArray(length);
    };
    const releaseCanvasRegion = region => {
        if (canvasPixelLength(region) <= MAX_CANVAS_REGION_SCRATCH_BYTES)
            canvasRegionScratch = region;
    };
    const paintCanvasRegion = (state, left, top, right, bottom, paint) => {
        const width = right - left, height = bottom - top, length = width * height * 4;
        const region = takeCanvasRegion(length);
        try {
            // Full-width rows are contiguous even when top is not zero. Use
            // one captured typed-array copy rather than one temporary view per row.
            if (width === state.width) {
                copyCanvasPixelRow(region, 0, state.pixels, top * width * 4, length);
            } else {
                for (let row = 0; row < height; row++)
                    copyCanvasPixelRow(region, row * width * 4, state.pixels,
                        ((row + top) * state.width + left) * 4, width * 4);
            }
            const painted = paint(region);
            if (!painted || canvasPixelLength(painted) !== length) return false;
            if (width === state.width) {
                copyCanvasPixelRow(state.pixels, top * width * 4, painted, 0, length);
            } else {
                for (let row = 0; row < height; row++)
                    copyCanvasPixelRow(state.pixels, ((row + top) * state.width + left) * 4,
                        painted, row * width * 4, width * 4);
            }
            return true;
        } finally {
            releaseCanvasRegion(region);
        }
    };
