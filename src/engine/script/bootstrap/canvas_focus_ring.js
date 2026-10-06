    // HTML CanvasUserInterface: draw only for a focused fallback descendant.
    // Platform focus ink ignores author stroke, alpha, dash and compositing state,
    // while the current clipping region continues to apply.
    // https://html.spec.whatwg.org/multipage/canvas.html#drawing-focus-rings
    CanvasRenderingContext2D.prototype.drawFocusIfNeeded = function(pathOrElement, element) {
        const external = pathOrElement instanceof Path2D;
        const target = external ? element : pathOrElement;
        if (!(target instanceof Element))
            throw new TypeError('drawFocusIfNeeded requires an Element');
        if (target !== focusedAreaForDocument(document) || !this.canvas.contains(target)) return;
        const path = canvasPathArgument(this, external ? pathOrElement : undefined);
        if (!path.subpaths.some(part => part.points.length > 1)) return;
        const previous = {
            stroke: canvasDrawingState(this).stroke, width: canvasDrawingState(this).lineWidth, dash: canvasDrawingState(this).lineDash,
            offset: canvasDrawingState(this).dashOffset, alpha: canvasDrawingState(this).globalAlpha,
            composite: canvasDrawingState(this).compositeOperation, cap: canvasDrawingState(this).lineCap,
            join: canvasDrawingState(this).lineJoin, miter: canvasDrawingState(this).miterLimit
        };
        try {
            canvasDrawingState(this).stroke = normalizedColor('#005fcc');
            canvasDrawingState(this).lineWidth = 2;
            canvasDrawingState(this).lineDash = [];
            canvasDrawingState(this).dashOffset = 0;
            canvasDrawingState(this).globalAlpha = 1;
            canvasDrawingState(this).compositeOperation = 'source-over';
            canvasDrawingState(this).lineCap = 'round';
            canvasDrawingState(this).lineJoin = 'round';
            canvasDrawingState(this).miterLimit = 1;
            paintCanvasPath(this, path, false, 'nonzero');
        } finally {
            canvasDrawingState(this).stroke = previous.stroke;
            canvasDrawingState(this).lineWidth = previous.width;
            canvasDrawingState(this).lineDash = previous.dash;
            canvasDrawingState(this).dashOffset = previous.offset;
            canvasDrawingState(this).globalAlpha = previous.alpha;
            canvasDrawingState(this).compositeOperation = previous.composite;
            canvasDrawingState(this).lineCap = previous.cap;
            canvasDrawingState(this).lineJoin = previous.join;
            canvasDrawingState(this).miterLimit = previous.miter;
        }
    };
