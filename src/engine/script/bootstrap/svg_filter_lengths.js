    // SVG 2 length reflection. Animated values are distinct read-only views;
    // base values write the original content attribute so filter rasterization
    // and later attribute reads agree with the DOM.
    // https://svgwg.org/svg2-draft/types.html#InterfaceSVGLength
    const svgLengthUnits = ['', '', '%', 'em', 'ex', 'px', 'cm', 'mm', 'in', 'pt', 'pc'];
    const svgLengthPattern = /^([+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?)(%|em|ex|px|cm|mm|in|pt|pc)?$/;
    const parseSvgLength = source => {
        const match = svgLengthPattern.exec(String(source).trim());
        if (!match) return null;
        const value = Number(match[1]);
        return Number.isFinite(value)
            ? { value, unit: match[2] ? svgLengthUnits.indexOf(match[2]) : 1 } : null;
    };
    const svgLengthReference = (element, axis) => {
        const filter = element.localName === 'filter' ? element : element.parentElement;
        const unitAttribute = element.localName === 'filter' ? 'filterUnits' : 'primitiveUnits';
        const objectBox = filter?.getAttribute(unitAttribute) === 'objectBoundingBox' ||
            (unitAttribute === 'filterUnits' && filter?.getAttribute(unitAttribute) !== 'userSpaceOnUse');
        if (objectBox) return 1;
        const root = element.ownerSVGElement;
        const dimension = root?.getAttribute(axis === 'x' ? 'width' : 'height');
        const parsed = dimension && parseSvgLength(dimension);
        return parsed?.unit === 1 || parsed?.unit === 5 ? parsed.value : axis === 'x' ? 300 : 150;
    };
    const svgLengthScale = (element, axis, unit) => {
        switch (unit) {
        case 1: case 5: return 1;
        case 2: return svgLengthReference(element, axis) / 100;
        case 3: case 4: {
            const fontSize = parseFloat(getComputedStyle(element).fontSize) || 16;
            return fontSize * (unit === 4 ? 0.5 : 1);
        }
        case 6: return 96 / 2.54;
        case 7: return 96 / 25.4;
        case 8: return 96;
        case 9: return 96 / 72;
        case 10: return 16;
        default: return 1;
        }
    };
    class SVGLength {
        constructor(token, element, attribute, fallback, axis, readOnly = false) {
            if (token !== svgFilterToken) throw new TypeError('Illegal constructor');
            this.__element = element;
            this.__attribute = attribute;
            this.__fallback = fallback;
            this.__axis = axis;
            this.__readOnly = readOnly;
        }
        __parsed() {
            return parseSvgLength(this.__element.getAttribute(this.__attribute) ?? this.__fallback)
                ?? parseSvgLength(this.__fallback);
        }
        __writable() {
            if (this.__readOnly)
                throw new DOMException('Animated SVG lengths are read-only', 'NoModificationAllowedError');
        }
        get unitType() { return this.__parsed()?.unit ?? 0; }
        get value() {
            const parsed = this.__parsed();
            return parsed.value * svgLengthScale(this.__element, this.__axis, parsed.unit);
        }
        set value(value) {
            this.__writable();
            value = Number(value);
            if (!Number.isFinite(value)) throw new TypeError('SVG length must be finite');
            const unit = this.unitType || 1;
            const scale = svgLengthScale(this.__element, this.__axis, unit);
            this.__element.setAttribute(this.__attribute,
                String(value / scale) + svgLengthUnits[unit]);
        }
        get valueInSpecifiedUnits() { return this.__parsed().value; }
        set valueInSpecifiedUnits(value) {
            this.__writable();
            value = Number(value);
            if (!Number.isFinite(value)) throw new TypeError('SVG length must be finite');
            this.__element.setAttribute(this.__attribute,
                String(value) + svgLengthUnits[this.unitType || 1]);
        }
        get valueAsString() {
            return this.__element.getAttribute(this.__attribute) ?? this.__fallback;
        }
        set valueAsString(value) {
            this.__writable();
            this.__element.setAttribute(this.__attribute, String(value));
        }
        newValueSpecifiedUnits(unit, value) {
            this.__writable();
            unit = Number(unit) >>> 0;
            value = Number(value);
            if (unit < 1 || unit >= svgLengthUnits.length || !Number.isFinite(value))
                throw new TypeError('Invalid SVG length');
            this.__element.setAttribute(this.__attribute, String(value) + svgLengthUnits[unit]);
        }
        convertToSpecifiedUnits(unit) {
            this.__writable();
            const value = this.value;
            unit = Number(unit) >>> 0;
            if (unit < 1 || unit >= svgLengthUnits.length)
                throw new TypeError('Invalid SVG length unit');
            this.newValueSpecifiedUnits(unit,
                value / svgLengthScale(this.__element, this.__axis, unit));
        }
    }
    for (const [name, value] of Object.entries({
        SVG_LENGTHTYPE_UNKNOWN: 0, SVG_LENGTHTYPE_NUMBER: 1,
        SVG_LENGTHTYPE_PERCENTAGE: 2, SVG_LENGTHTYPE_EMS: 3,
        SVG_LENGTHTYPE_EXS: 4, SVG_LENGTHTYPE_PX: 5,
        SVG_LENGTHTYPE_CM: 6, SVG_LENGTHTYPE_MM: 7,
        SVG_LENGTHTYPE_IN: 8, SVG_LENGTHTYPE_PT: 9,
        SVG_LENGTHTYPE_PC: 10
    })) {
        Object.defineProperty(SVGLength, name, { enumerable: true, value });
        Object.defineProperty(SVGLength.prototype, name, { enumerable: true, value });
    }
    class SVGAnimatedLength {
        constructor(token, element, attribute, fallback, axis) {
            if (token !== svgFilterToken) throw new TypeError('Illegal constructor');
            this.__base = new SVGLength(token, element, attribute, fallback, axis);
            this.__animated = new SVGLength(token, element, attribute, fallback, axis, true);
        }
        get baseVal() { return this.__base; }
        get animVal() { return this.__animated; }
    }
