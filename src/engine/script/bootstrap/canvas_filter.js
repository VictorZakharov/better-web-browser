    // CSS filter admission shares the native tokenizer and typed value parsers.
    // Resolve relative units and currentColor at assignment, not at draw time.
    Object.defineProperty(CanvasRenderingContext2D.prototype, 'filter', {
        configurable:true, enumerable:true,
        get() { return canvasDrawingState(this).filter; },
        set(value) {
            const state = canvasDrawingState(this);
            const text = `${value}`;
            const operations = host('canvasFilterParse',
                canvasNativeElement(this.canvas, 'canvas') ?
                    canvasOwnerWeakGet(nodeHandles, this.canvas) : 0, text);
            if (operations) { state.filter = text; state.filterOperations = operations; }
        }
    });
    const canvasFilterBlur = (pixels, width, height, sigma) => {
        if (sigma < 0.01) return pixels;
        const result = canvasCompositeLayerHost('canvasFilterGaussian', pixels,
            canvasPrivateWireStringify({width, height, sigma}));
        if (!result || canvasPixelLength(result) !== canvasPrivateCount(pixels))
            throw new DOMException('Canvas filter exceeds the bounded Gaussian working set',
                'NotSupportedError');
        return new canvasPrivatePixelArray(result);
    };
    const canvasFilterColors = (pixels, width, height, operations) => {
        const result = canvasCompositeLayerHost('canvasFilterColor', pixels,
            canvasPrivateWireStringify({width, height, operations}));
        if (!result || canvasPixelLength(result) !== canvasPrivateCount(pixels))
            throw new DOMException('Canvas filter exceeds the bounded color working set',
                'NotSupportedError');
        return new canvasPrivatePixelArray(result);
    };
    const applyCanvasFilters = (pixels, width, height, operations) => {
        let result = pixels;
        for (let index = 0; index < operations.length; index++) {
            const {name, value} = operations[index];
            if (name === 'blur') { result = canvasFilterBlur(result, width, height, value); continue; }
            if (name === 'drop-shadow') {
                const blurred = canvasFilterBlur(result, width, height, value.blur);
                const shadow = canvasShadowLayerFromState({shadowBlur: 0,
                    shadowOffsetX: value.x, shadowOffsetY: value.y,
                    shadowColor: value.color}, blurred, width, height);
                for (let offset = 0; offset < canvasPrivateCount(result); offset += 4)
                    compositeCanvasPixelAt(shadow, offset, result, offset, 1, 'source-over');
                result = shadow;
                continue;
            }
            // Never expose private records to replaceable array iteration,
            // species, or inherited toJSON callbacks.
            const colors = [];
            canvasPrivatePush(colors, operations[index]);
            while (index + 1 < operations.length && operations[index + 1].name !== 'blur' &&
                operations[index + 1].name !== 'drop-shadow')
                canvasPrivatePush(colors, operations[++index]);
            result = canvasFilterColors(result, width, height, colors);
        }
        return result;
    };
