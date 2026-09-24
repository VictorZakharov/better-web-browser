    if (host('canvasTextAvailable')) {
    // HTML Canvas text uses the same OpenType shaping backend as page layout.
    // https://html.spec.whatwg.org/multipage/canvas.html#text-styles
    const canvasTextAlignments = new Set(['start', 'end', 'left', 'right', 'center']);
    const canvasTextBaselines = new Set(['top', 'hanging', 'middle', 'alphabetic',
        'ideographic', 'bottom']);
    const canvasTextDirections = new Set(['inherit', 'ltr', 'rtl']);
    Object.defineProperties(CanvasRenderingContext2D.prototype, {
        font: {
            get() { return this.__font; },
            set(value) {
                const serialized = String(value);
                const spec = host('canvasParseFont', serialized);
                if (spec) { this.__font = serialized; this.__fontSpec = spec; }
            }
        },
        textAlign: {
            get() { return this.__textAlign; },
            set(value) { if (canvasTextAlignments.has(value)) this.__textAlign = value; }
        },
        textBaseline: {
            get() { return this.__textBaseline; },
            set(value) { if (canvasTextBaselines.has(value)) this.__textBaseline = value; }
        },
        direction: {
            get() { return this.__direction; },
            set(value) { if (canvasTextDirections.has(value)) this.__direction = value; }
        }
    });
    const canvasTextDirection = context => context.__direction === 'inherit' ?
        (context.canvas.ownerDocument?.documentElement?.getAttribute('dir') === 'rtl' ?
            'rtl' : 'ltr') : context.__direction;
    const canvasTextShift = (context, width) => {
        const align = context.__textAlign;
        const direction = canvasTextDirection(context);
        if (align === 'center') return -width / 2;
        if (align === 'right' || (align === 'end' && direction === 'ltr') ||
            (align === 'start' && direction === 'rtl')) return -width;
        return 0;
    };
    const canvasBaselineShift = (context, ascent, descent) => {
        switch (context.__textBaseline) {
            case 'top': return ascent;
            case 'hanging': return ascent * 0.8;
            case 'middle': return (ascent - descent) / 2;
            case 'ideographic': return -descent * 0.5;
            case 'bottom': return -descent;
            default: return 0;
        }
    };
    const canvasTextValue = text => String(text).replace(/[\u0009-\u000d]/g, ' ');
    const canvasTextRun = (context, text, stroke = false) => host(
        stroke ? 'canvasStrokeText' : 'canvasRasterText',
        canvasTextValue(text), context.__fontSpec, context.__lineWidth);
    const canvasTextInkBounds = glyphs => {
        let left = Infinity, top = Infinity, right = -Infinity, bottom = -Infinity;
        for (const glyph of glyphs) {
            left = Math.min(left, glyph[0]); top = Math.min(top, glyph[1]);
            right = Math.max(right, glyph[0] + glyph[2]);
            bottom = Math.max(bottom, glyph[1] + glyph[3]);
        }
        return glyphs.length ? [left, top, right, bottom] : [0, 0, 0, 0];
    };
    CanvasRenderingContext2D.prototype.measureText = function(text) {
        if (arguments.length === 0) throw new TypeError('measureText requires text');
        const [width, ascent, descent, glyphs] = canvasTextRun(this, text);
        const shift = canvasTextShift(this, width);
        const [left, top, right, bottom] = canvasTextInkBounds(glyphs);
        const metrics = {
            width,
            actualBoundingBoxLeft: -(left + shift),
            actualBoundingBoxRight: right + shift,
            actualBoundingBoxAscent: Math.max(0, -top),
            actualBoundingBoxDescent: Math.max(0, bottom),
            fontBoundingBoxAscent: ascent,
            fontBoundingBoxDescent: descent,
            emHeightAscent: ascent,
            emHeightDescent: descent,
            hangingBaseline: ascent * 0.8,
            alphabeticBaseline: 0,
            ideographicBaseline: -descent * 0.5
        };
        return Object.assign(Object.create(TextMetrics.prototype), metrics);
    };
    class TextMetrics {}
    Object.defineProperty(TextMetrics.prototype, Symbol.toStringTag, { value: 'TextMetrics' });
    globalThis.TextMetrics = TextMetrics;

    const canvasTextPaint = function(text, x, y, maxWidth, stroke) {
        const state = stateForCanvas(this.canvas);
        if (!state.pixels || !Number.isFinite(x) || !Number.isFinite(y)) return;
        const [width, ascent, descent, glyphs] = canvasTextRun(this, text, stroke);
        if (!glyphs.length) return;
        const scale = maxWidth === undefined || width <= maxWidth ? 1 : maxWidth / width;
        const shift = canvasTextShift(this, width) * scale;
        const baselineShift = canvasBaselineShift(this, ascent, descent);
        const inverse = matrixInverse2D(this.__transform);
        if (!inverse) return;
        for (const [left, top, glyphWidth, glyphHeight, colorGlyph, data] of glyphs) {
            const gx = x + shift + left * scale, gy = y + baselineShift + top;
            const bounds = canvasTransformedBounds(this.__transform, gx, gy,
                glyphWidth * scale, glyphHeight, state);
            if (!bounds) continue;
            const [minX, minY, maxX, maxY] = bounds;
            for (let py = minY; py < maxY; py++) for (let px = minX; px < maxX; px++) {
                if (!canvasClipAllows(this, px, py, state.width)) continue;
                const [ux, uy] = matrixPoint2D(inverse, px + 0.5, py + 0.5);
                const sx = Math.floor((ux - gx) / scale), sy = Math.floor(uy - gy);
                if (sx < 0 || sy < 0 || sx >= glyphWidth || sy >= glyphHeight) continue;
                const source = sy * glyphWidth + sx;
                const coverage = colorGlyph ? data[source * 4 + 3] : data[source];
                if (!coverage) continue;
                const paint = colorGlyph && !stroke ? data.subarray(source * 4, source * 4 + 4) :
                    canvasPaintAt(stroke ? this.__stroke : this.__fill,
                        px + 0.5, py + 0.5, inverse);
                const rgba = colorGlyph ? paint : [paint[0], paint[1], paint[2],
                    paint[3] * coverage / 255];
                compositeCanvasPixel(state.pixels, (py * state.width + px) * 4,
                    rgba, this.__globalAlpha, this.__compositeOperation);
            }
        }
    };
    CanvasRenderingContext2D.prototype.fillText = function(text, x, y, maxWidth) {
        if (arguments.length < 3) throw new TypeError('fillText requires text and coordinates');
        x = Number(x); y = Number(y);
        if (maxWidth !== undefined) {
            maxWidth = Number(maxWidth);
            if (!Number.isFinite(maxWidth) || maxWidth <= 0) return;
        }
        return canvasCompositeSourceLayer(this, canvasTextPaint, [text, x, y, maxWidth, false]);
    };
    CanvasRenderingContext2D.prototype.strokeText = function(text, x, y, maxWidth) {
        if (arguments.length < 3) throw new TypeError('strokeText requires text and coordinates');
        x = Number(x); y = Number(y);
        if (maxWidth !== undefined) {
            maxWidth = Number(maxWidth);
            if (!Number.isFinite(maxWidth) || maxWidth <= 0) return;
        }
        return canvasCompositeSourceLayer(this, canvasTextPaint, [text, x, y, maxWidth, true]);
    };
    }
