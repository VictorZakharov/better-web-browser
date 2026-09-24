    // HTML CanvasUserInterface: draw only for a focused fallback descendant.
    // Platform focus ink ignores author stroke, alpha, dash and compositing state,
    // while the current clipping region continues to apply.
    // https://html.spec.whatwg.org/multipage/canvas.html#drawing-focus-rings
    CanvasRenderingContext2D.prototype.drawFocusIfNeeded = function(pathOrElement, element) {
        const external = pathOrElement instanceof Path2D;
        const target = external ? element : pathOrElement;
        if (!(target instanceof Element))
            throw new TypeError('drawFocusIfNeeded requires an Element');
        if (target !== document.activeElement || !this.canvas.contains(target)) return;
        const path = canvasPathArgument(this, external ? pathOrElement : undefined);
        if (!path.subpaths.some(part => part.points.length > 1)) return;
        const previous = {
            stroke: this.__stroke, width: this.__lineWidth, dash: this.__lineDash,
            offset: this.__dashOffset, alpha: this.__globalAlpha,
            composite: this.__compositeOperation, cap: this.__lineCap,
            join: this.__lineJoin, miter: this.__miterLimit
        };
        try {
            this.__stroke = normalizedColor('#005fcc');
            this.__lineWidth = 2;
            this.__lineDash = [];
            this.__dashOffset = 0;
            this.__globalAlpha = 1;
            this.__compositeOperation = 'source-over';
            this.__lineCap = 'round';
            this.__lineJoin = 'round';
            this.__miterLimit = 1;
            paintCanvasPath(this, path, false, 'nonzero');
        } finally {
            this.__stroke = previous.stroke;
            this.__lineWidth = previous.width;
            this.__lineDash = previous.dash;
            this.__dashOffset = previous.offset;
            this.__globalAlpha = previous.alpha;
            this.__compositeOperation = previous.composite;
            this.__lineCap = previous.cap;
            this.__lineJoin = previous.join;
            this.__miterLimit = previous.miter;
        }
    };
