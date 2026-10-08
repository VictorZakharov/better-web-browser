    if (host('canvasTextAvailable')) {
    // HTML Canvas text uses the same OpenType shaping backend as page layout.
    // https://html.spec.whatwg.org/multipage/canvas.html#text-styles
    const canvasTextAlignments = new Set(['start', 'end', 'left', 'right', 'center']);
    const canvasTextBaselines = new Set(['top', 'hanging', 'middle', 'alphabetic',
        'ideographic', 'bottom']);
    const canvasTextDirections = new Set(['inherit', 'ltr', 'rtl']);
    const canvasSpacingStyles = {};
    for (const name of ['letterSpacing', 'wordSpacing']) {
        canvasSpacingStyles[name] = {
            get() { return canvasDrawingState(this)[name][0]; },
            set(value) {
                const parsed = host('canvasParseSpacing', `${value}`);
                if (parsed) canvasDrawingState(this)[name] = parsed;
            }
        };
    }
    defineCanvasContextProperties(CanvasRenderingContext2D.prototype, canvasSpacingStyles);
    defineCanvasContextProperties(CanvasRenderingContext2D.prototype, {
        font: {
            get() { return canvasDrawingState(this).font; },
            set(value) {
                const serialized = `${value}`;
                const spec = host('canvasParseFont', serialized);
                if (spec) { canvasDrawingState(this).font = serialized; canvasDrawingState(this).fontSpec = spec; }
            }
        },
        textAlign: {
            get() { return canvasDrawingState(this).textAlign; },
            set(value) { value = `${value}`; if (canvasTextAlignments.has(value)) canvasDrawingState(this).textAlign = value; }
        },
        textBaseline: {
            get() { return canvasDrawingState(this).textBaseline; },
            set(value) { value = `${value}`; if (canvasTextBaselines.has(value)) canvasDrawingState(this).textBaseline = value; }
        },
        direction: {
            get() { return canvasDrawingState(this).direction; },
            set(value) { value = `${value}`; if (canvasTextDirections.has(value)) canvasDrawingState(this).direction = value; }
        },
        fontKerning: {
            get() { return canvasDrawingState(this).fontKerning; },
            set(value) {
                value = `${value}`;
                if (['auto','normal','none'].includes(value)) canvasDrawingState(this).fontKerning = value;
            }
        },
        lang: {
            get() { return canvasDrawingState(this).lang; },
            set(value) { canvasDrawingState(this).lang = `${value}`; }
        }
    });
    const canvasTextEnvironment = context => host('canvasTextEnvironment',
        canvasNativeElement(context.canvas, 'canvas') ? canvasOwnerWeakGet(nodeHandles, context.canvas) : 0,
        !!(canvasDrawingState(context).letterSpacing[1][2] || canvasDrawingState(context).wordSpacing[1][2]));
    const canvasTextDirection = context => canvasDrawingState(context).direction === 'inherit' ?
        canvasTextEnvironment(context)[3] : canvasDrawingState(context).direction;
    const canvasTextShift = (context, width) => {
        const align = canvasDrawingState(context).textAlign;
        const direction = canvasTextDirection(context);
        if (align === 'center') return -width / 2;
        if (align === 'right' || (align === 'end' && direction === 'ltr') ||
            (align === 'start' && direction === 'rtl')) return -width;
        return 0;
    };
    const canvasBaselineShift = (context, ascent, descent) => {
        switch (canvasDrawingState(context).textBaseline) {
            case 'top': return ascent;
            case 'hanging': return ascent * 0.8;
            case 'middle': return (ascent - descent) / 2;
            case 'ideographic': return -descent * 0.5;
            case 'bottom': return -descent;
            default: return 0;
        }
    };
    const canvasTextValue = text => `${text}`.replace(/[\u0009-\u000d]/g, ' ');
    const canvasTextSpacing = (context, terms, environment) => {
        const fontSize = canvasDrawingState(context).fontSpec[1];
        const [width, height, rootSize] = environment;
        return terms[0] + terms[1] * fontSize + terms[2] * rootSize +
            (terms[3] * width + terms[4] * height + terms[5] * canvasPrivateMath.min(width, height) +
                terms[6] * canvasPrivateMath.max(width, height)) / 100;
    };
    const canvasTextRun = (context, text, stroke = false) => {
        text = canvasTextValue(text);
        const state = canvasDrawingState(context);
        prepareCanvasFontFaces(state.font, text);
        const environment = canvasTextEnvironment(context);
        const spec = [...state.fontSpec, canvasTextSpacing(context, state.letterSpacing[1], environment),
            canvasTextSpacing(context, state.wordSpacing[1], environment)];
        const language = state.lang === 'inherit' ? environment[4] : state.lang;
        return host(stroke ? 'canvasStrokeText' : 'canvasRasterText',
            text, spec, state.lineWidth,
            [state.direction === 'inherit' ? environment[3] : state.direction, language, state.fontKerning]);
    };
    const canvasTextInkBounds = glyphs => {
        let left = Infinity, top = Infinity, right = -Infinity, bottom = -Infinity;
        for (const glyph of glyphs) {
            left = canvasPrivateMath.min(left, glyph[0]); top = canvasPrivateMath.min(top, glyph[1]);
            right = canvasPrivateMath.max(right, glyph[0] + glyph[2]);
            bottom = canvasPrivateMath.max(bottom, glyph[1] + glyph[3]);
        }
        return glyphs.length ? [left, top, right, bottom] : [0, 0, 0, 0];
    };
    CanvasRenderingContext2D.prototype.measureText = function(text) {
        if (arguments.length === 0) throw new TypeError('measureText requires text');
        const [width, ascent, descent, glyphs] = canvasTextRun(this, text);
        const shift = canvasTextShift(this, width);
        const baselineShift = canvasBaselineShift(this, ascent, descent);
        const [left, top, right, bottom] = canvasTextInkBounds(glyphs);
        const metrics = {
            width,
            actualBoundingBoxLeft: glyphs.length ? -(left + shift) : 0,
            actualBoundingBoxRight: glyphs.length ? right + shift : 0,
            actualBoundingBoxAscent: glyphs.length ? -top - baselineShift : 0,
            actualBoundingBoxDescent: glyphs.length ? bottom + baselineShift : 0,
            fontBoundingBoxAscent: ascent - baselineShift,
            fontBoundingBoxDescent: descent + baselineShift,
            emHeightAscent: ascent - baselineShift,
            emHeightDescent: descent + baselineShift,
            hangingBaseline: ascent * 0.8 - baselineShift,
            alphabeticBaseline: -baselineShift,
            ideographicBaseline: -descent * 0.5 - baselineShift
        };
        return new TextMetrics(canvasTextMetricsToken, metrics);
    };
    const canvasTextMetricsToken = Symbol('TextMetrics');
    const canvasTextMetricsStates = new WeakMap();
    class TextMetrics {
        constructor(token, metrics) {
            if (token !== canvasTextMetricsToken) throw new TypeError('Illegal constructor');
            canvasDrawingWeakSet(canvasTextMetricsStates, this, metrics);
        }
    }
    for (const name of ['width','actualBoundingBoxLeft','actualBoundingBoxRight',
        'actualBoundingBoxAscent','actualBoundingBoxDescent','fontBoundingBoxAscent',
        'fontBoundingBoxDescent','emHeightAscent','emHeightDescent','hangingBaseline',
        'alphabeticBaseline','ideographicBaseline']) {
        canvasPathDefine(TextMetrics.prototype, name, {enumerable:true, configurable:true,
            get() {
                const metrics = canvasDrawingWeakGet(canvasTextMetricsStates, this);
                if (!metrics) throw new TypeError('Illegal TextMetrics receiver');
                return metrics[name];
            }
        });
    }
    canvasPathDefine(TextMetrics, 'length', {value:0, configurable:true});
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
        const inverse = matrixInverse2D(canvasDrawingState(this).transform);
        if (!inverse) return;
        const paint = stroke ? canvasDrawingState(this).stroke : canvasDrawingState(this).fill;
        if (canvasPaintGlyphs(this, state, glyphs, paint, x+shift, y+baselineShift, scale, inverse, stroke))
            return;
        for (const [left, top, glyphWidth, glyphHeight, colorGlyph, data] of glyphs) {
            const gx = x + shift + left * scale, gy = y + baselineShift + top;
            const bounds = canvasTransformedBounds(canvasDrawingState(this).transform, gx, gy,
                glyphWidth * scale, glyphHeight, state);
            if (!bounds) continue;
            const [minX, minY, maxX, maxY] = bounds;
            for (let py = minY; py < maxY; py++) for (let px = minX; px < maxX; px++) {
                if (!canvasClipAllows(this, px, py, state.width)) continue;
                const [ux, uy] = matrixPoint2D(inverse, px + 0.5, py + 0.5);
                const sx = canvasPrivateMath.floor((ux - gx) / scale), sy = canvasPrivateMath.floor(uy - gy);
                if (sx < 0 || sy < 0 || sx >= glyphWidth || sy >= glyphHeight) continue;
                const source = sy * glyphWidth + sx;
                const coverage = colorGlyph ? data[source * 4 + 3] : data[source];
                if (!coverage) continue;
                const paint = colorGlyph && !stroke ? data.subarray(source * 4, source * 4 + 4) :
                    canvasPaintAt(stroke ? canvasDrawingState(this).stroke : canvasDrawingState(this).fill,
                        px + 0.5, py + 0.5, inverse);
                const rgba = colorGlyph ? paint : [paint[0], paint[1], paint[2],
                    paint[3] * coverage / 255];
                compositeCanvasPixel(state.pixels, (py * state.width + px) * 4,
                    rgba, canvasDrawingState(this).globalAlpha, canvasDrawingState(this).compositeOperation);
            }
        }
    };
    CanvasRenderingContext2D.prototype.fillText = function(text, x, y, maxWidth) {
        if (arguments.length < 3) throw new TypeError('fillText requires text and coordinates');
        text = canvasTextValue(text); x = +x; y = +y;
        if (maxWidth !== undefined) {
            maxWidth = +maxWidth;
            if (!Number.isFinite(maxWidth) || maxWidth <= 0) return;
        }
        if (!text || !Number.isFinite(x) || !Number.isFinite(y)) return;
        return canvasCompositeSourceLayer(this, canvasTextPaint, [text, x, y, maxWidth, false]);
    };
    CanvasRenderingContext2D.prototype.strokeText = function(text, x, y, maxWidth) {
        if (arguments.length < 3) throw new TypeError('strokeText requires text and coordinates');
        text = canvasTextValue(text); x = +x; y = +y;
        if (maxWidth !== undefined) {
            maxWidth = +maxWidth;
            if (!Number.isFinite(maxWidth) || maxWidth <= 0) return;
        }
        if (!text || !Number.isFinite(x) || !Number.isFinite(y)) return;
        return canvasCompositeSourceLayer(this, canvasTextPaint, [text, x, y, maxWidth, true]);
    };
    }
