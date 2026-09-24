    // Filter Effects Appendix B: expose DOM interfaces only for filter operations
    // rendered by the SVG rasterizer. Reflected values modify the backing SVG
    // attributes, so subsequent paints observe them.
    // https://drafts.csswg.org/filter-effects-1/#dom-interfaces
    class SVGAnimatedNumber {
        constructor(token, element, attribute, fallback = 0, position = 0) {
            if (token !== svgFilterToken) throw new TypeError('Illegal constructor');
            this.__element = element;
            this.__attribute = attribute;
            this.__fallback = fallback;
            this.__position = position;
        }
        get baseVal() {
            const source = this.__element.getAttribute(this.__attribute);
            if (source === null) return this.__fallback;
            const numbers = parseSvgNumberList(source);
            return numbers[this.__position] ??
                (this.__position === 1 ? numbers[0] : undefined) ?? this.__fallback;
        }
        set baseVal(value) {
            value = Number(value);
            if (!Number.isFinite(value)) throw new TypeError('SVG number must be finite');
            if (this.__position === 0 && this.__attribute !== 'stdDeviation') {
                this.__element.setAttribute(this.__attribute, String(value));
                return;
            }
            const source = this.__element.getAttribute(this.__attribute);
            const numbers = source === null ? [] : parseSvgNumberList(source);
            const x = this.__position === 0 ? value : numbers[0] ?? this.__fallback;
            const y = this.__position === 1 ? value : numbers[1] ?? numbers[0] ?? this.__fallback;
            this.__element.setAttribute(this.__attribute, `${x} ${y}`);
        }
        get animVal() { return this.baseVal; }
    }
    const filterAnimatedString = (element, name) =>
        element['__svg_' + name] ||= new SVGAnimatedString(element, name);
    const filterAnimatedNumber = (element, name, fallback = 0, position = 0) =>
        element['__svg_' + name + '_' + position] ||=
            new SVGAnimatedNumber(svgFilterToken, element, name, fallback, position);
    const filterAnimatedEnum = (element, name, values, fallback = 1) =>
        element['__svg_' + name] ||=
            new SVGAnimatedEnumeration(svgFilterToken, element, name, values, fallback);
    const filterIn = prototype => Object.defineProperty(prototype, 'in1', {
        enumerable: true, get() { return filterAnimatedString(this, 'in'); }
    });
    const filterIn2 = prototype => Object.defineProperty(prototype, 'in2', {
        enumerable: true, get() { return filterAnimatedString(this, 'in2'); }
    });
    const filterNumber = (prototype, property, attribute = property, fallback = 0, position = 0) =>
        Object.defineProperty(prototype, property, {
            enumerable: true,
            get() { return filterAnimatedNumber(this, attribute, fallback, position); }
        });
    const filterEnum = (prototype, property, attribute, values, fallback = 1) =>
        Object.defineProperty(prototype, property, {
            enumerable: true,
            get() { return filterAnimatedEnum(this, attribute, values, fallback); }
        });
    const filterConstants = (interface_, prefix, values) => {
        for (const [index, token] of values.entries()) {
            const name = prefix + token.toUpperCase().replaceAll('-', '_');
            Object.defineProperty(interface_, name, { enumerable: true, value: index });
            Object.defineProperty(interface_.prototype, name, { enumerable: true, value: index });
        }
    };
    const primitiveResult = prototype => Object.defineProperty(prototype, 'result', {
        enumerable: true, get() { return filterAnimatedString(this, 'result'); }
    });
    const filterLength = (prototype, name, fallback, axis) =>
        Object.defineProperty(prototype, name, {
            enumerable: true,
            get() {
                return this['__svg_length_' + name] ||= new SVGAnimatedLength(
                    svgFilterToken, this, name, fallback, axis);
            }
        });
    class SVGUnitTypes {
        constructor() { throw new TypeError('Illegal constructor'); }
    }
    for (const [name, value] of Object.entries({
        SVG_UNIT_TYPE_UNKNOWN: 0,
        SVG_UNIT_TYPE_OBJECTBOUNDINGBOX: 1,
        SVG_UNIT_TYPE_USERSPACEONUSE: 2
    })) {
        Object.defineProperty(SVGUnitTypes, name, { enumerable: true, value });
        Object.defineProperty(SVGUnitTypes.prototype, name, { enumerable: true, value });
    }
    class SVGFilterElement extends SVGElement {}
    filterEnum(SVGFilterElement.prototype, 'filterUnits', 'filterUnits',
        ['unknown', 'objectBoundingBox', 'userSpaceOnUse']);
    filterEnum(SVGFilterElement.prototype, 'primitiveUnits', 'primitiveUnits',
        ['unknown', 'objectBoundingBox', 'userSpaceOnUse'], 2);
    for (const [name, fallback, axis] of [
        ['x', '-10%', 'x'], ['y', '-10%', 'y'],
        ['width', '120%', 'x'], ['height', '120%', 'y']
    ]) filterLength(SVGFilterElement.prototype, name, fallback, axis);

    class SVGFEOffsetElement extends SVGElement {}
    filterIn(SVGFEOffsetElement.prototype);
    filterNumber(SVGFEOffsetElement.prototype, 'dx');
    filterNumber(SVGFEOffsetElement.prototype, 'dy');

    class SVGFEGaussianBlurElement extends SVGElement {
        setStdDeviation(x, y) {
            x = Number(x); y = Number(y);
            if (!Number.isFinite(x) || !Number.isFinite(y))
                throw new TypeError('SVG deviations must be finite');
            this.setAttribute('stdDeviation', `${x} ${y}`);
        }
    }
    filterIn(SVGFEGaussianBlurElement.prototype);
    filterNumber(SVGFEGaussianBlurElement.prototype, 'stdDeviationX', 'stdDeviation');
    filterNumber(SVGFEGaussianBlurElement.prototype, 'stdDeviationY', 'stdDeviation', 0, 1);
    filterEnum(SVGFEGaussianBlurElement.prototype, 'edgeMode', 'edgeMode',
        ['unknown', 'duplicate', 'wrap', 'none'], 1);
    filterConstants(SVGFEGaussianBlurElement, 'SVG_EDGEMODE_',
        ['unknown', 'duplicate', 'wrap', 'none']);

    class SVGFECompositeElement extends SVGElement {}
    filterIn(SVGFECompositeElement.prototype);
    filterIn2(SVGFECompositeElement.prototype);
    filterEnum(SVGFECompositeElement.prototype, 'operator', 'operator',
        ['unknown', 'over', 'in', 'out', 'atop', 'xor', 'arithmetic']);
    filterConstants(SVGFECompositeElement, 'SVG_FECOMPOSITE_OPERATOR_',
        ['unknown', 'over', 'in', 'out', 'atop', 'xor', 'arithmetic']);
    for (const name of ['k1', 'k2', 'k3', 'k4']) filterNumber(SVGFECompositeElement.prototype, name);

    class SVGFEBlendElement extends SVGElement {}
    filterIn(SVGFEBlendElement.prototype);
    filterIn2(SVGFEBlendElement.prototype);
    const blendModes = ['unknown', 'normal', 'multiply', 'screen', 'darken', 'lighten',
        'overlay', 'color-dodge', 'color-burn', 'hard-light', 'soft-light', 'difference',
        'exclusion', 'hue', 'saturation', 'color', 'luminosity'];
    filterEnum(SVGFEBlendElement.prototype, 'mode', 'mode', blendModes);
    filterConstants(SVGFEBlendElement, 'SVG_FEBLEND_MODE_', blendModes);

    class SVGFEFloodElement extends SVGElement {}
    class SVGFEMergeElement extends SVGElement {}
    class SVGFEMergeNodeElement extends SVGElement {}
    filterIn(SVGFEMergeNodeElement.prototype);

    for (const interface_ of [SVGFEColorMatrixElement, SVGFEOffsetElement,
        SVGFEGaussianBlurElement, SVGFECompositeElement, SVGFEBlendElement,
        SVGFEFloodElement, SVGFEMergeElement]) {
        primitiveResult(interface_.prototype);
        for (const [name, fallback, axis] of [
            ['x', '0%', 'x'], ['y', '0%', 'y'],
            ['width', '100%', 'x'], ['height', '100%', 'y']
        ]) filterLength(interface_.prototype, name, fallback, axis);
    }

    const svgFilterElementInterfaces = Object.freeze({
        filter: SVGFilterElement,
        feColorMatrix: SVGFEColorMatrixElement,
        feOffset: SVGFEOffsetElement,
        feGaussianBlur: SVGFEGaussianBlurElement,
        feComposite: SVGFECompositeElement,
        feBlend: SVGFEBlendElement,
        feFlood: SVGFEFloodElement,
        feMerge: SVGFEMergeElement,
        feMergeNode: SVGFEMergeNodeElement
    });
