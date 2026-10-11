    // SVG 1.1 live rects remain the interoperable browser API (Chrome returns
    // SVGRect, not the replacement DOMRect described by SVG 2). The attribute
    // remains authoritative, so setters invalidate the existing SVG rasterizer.
    // https://www.w3.org/TR/SVG11/types.html#InterfaceSVGAnimatedRect
    const svgRectToken = Symbol('SVG rectangle');
    const svgRectGetAttribute = Element.prototype.getAttribute;
    const svgRectSetAttribute = Element.prototype.setAttribute;
    const svgRects = new WeakMap(), svgAnimatedRects = new WeakMap();
    const svgRectFields = ['x', 'y', 'width', 'height'];
    const svgViewBoxNumber = /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?/;
    function parseSvgViewBox(source) {
        source = String(source ?? '').trim();
        const result = [];
        for (let i = 0; i < 4; i++) {
            const match = svgViewBoxNumber.exec(source);
            if (!match) return [0, 0, 0, 0];
            const value = Math.fround(Number(match[0]));
            if (!Number.isFinite(value)) return [0, 0, 0, 0];
            result.push(value);
            source = source.slice(match[0].length);
            if (i === 3) break;
            const separator = /^[\t\n\r ]*(?:,[\t\n\r ]*)?/.exec(source)[0];
            source = source.slice(separator.length);
            // SVG number lists also admit an adjacent sign (e.g. 1-2).
            if (!separator && !/^[+-]/.test(source)) return [0, 0, 0, 0];
        }
        return source.trim() ? [0, 0, 0, 0] : result;
    }
    function svgRectState(rect) {
        const state = svgRects.get(rect);
        if (!state) throw new TypeError('Illegal invocation');
        if (state.element) state.values = parseSvgViewBox(svgRectGetAttribute.call(state.element, 'viewBox'));
        return state;
    }
    class SVGRect {
        constructor(token, element = null, readOnly = false) {
            if (token !== svgRectToken) throw new TypeError('Illegal constructor');
            svgRects.set(this, {element, readOnly, values: [0, 0, 0, 0]});
        }
    }
    svgRectFields.forEach((field, index) => Object.defineProperty(SVGRect.prototype, field, {
        configurable: true, enumerable: true,
        get() { return svgRectState(this).values[index]; },
        set(value) {
            const state = svgRectState(this);
            if (state.readOnly)
                throw new DOMException('Animated SVG rectangles are read-only', 'NoModificationAllowedError');
            value = Math.fround(Number(value));
            if (!Number.isFinite(value)) throw new TypeError('SVG rectangle coordinates must be finite');
            state.values[index] = value;
            if (state.element) svgRectSetAttribute.call(state.element, 'viewBox', state.values.join(' '));
        }
    }));
    Object.defineProperty(SVGRect.prototype, Symbol.toStringTag, {value: 'SVGRect', configurable: true});
    class SVGAnimatedRect {
        constructor(token, element) {
            if (token !== svgRectToken) throw new TypeError('Illegal constructor');
            svgAnimatedRects.set(this, {
                base: new SVGRect(token, element), animated: new SVGRect(token, element, true)
            });
        }
        get baseVal() {
            const state = svgAnimatedRects.get(this);
            if (!state) throw new TypeError('Illegal invocation');
            return state.base;
        }
        get animVal() {
            const state = svgAnimatedRects.get(this);
            if (!state) throw new TypeError('Illegal invocation');
            return state.animated;
        }
    }
    Object.defineProperty(SVGAnimatedRect.prototype, Symbol.toStringTag,
        {value: 'SVGAnimatedRect', configurable: true});
    const svgViewBoxes = new WeakMap();
    Object.defineProperty(SVGSVGElement.prototype, 'viewBox', {
        configurable: true, enumerable: true,
        get() {
            if (!(this instanceof SVGSVGElement)) throw new TypeError('Illegal invocation');
            let value = svgViewBoxes.get(this);
            if (!value) svgViewBoxes.set(this, value = new SVGAnimatedRect(svgRectToken, this));
            return value;
        }
    });
    SVGSVGElement.prototype.createSVGRect = function() {
        if (!(this instanceof SVGSVGElement)) throw new TypeError('Illegal invocation');
        return new SVGRect(svgRectToken);
    };
