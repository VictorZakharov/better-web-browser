    // Filter Effects §9.6 and SVG2 §4.6: the DOM attributes reflect the same
    // SVG markup that the rasterizer consumes. Mutating baseVal therefore
    // invalidates and rerenders the inline SVG, not a detached JS-only copy.
    // https://drafts.csswg.org/filter-effects-1/#InterfaceSVGFEColorMatrixElement
    const svgFilterToken = Symbol('SVG filter interface');
    const colorMatrixTypes = ['unknown', 'matrix', 'saturate', 'hueRotate', 'luminanceToAlpha'];
    const svgNumberPattern = /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/;
    const parseSvgNumberList = source => {
        const raw = source.trim();
        if (!raw) return [];
        const parts = raw.split(/[\s,]+/);
        return parts.every(part => svgNumberPattern.test(part)) ? parts.map(Number) : [];
    };
    class SVGAnimatedEnumeration {
        constructor(token, element, attribute, values, fallback) {
            if (token !== svgFilterToken) throw new TypeError('Illegal constructor');
            this.__element = element;
            this.__attribute = attribute;
            this.__values = values;
            this.__fallback = fallback;
        }
        get baseVal() {
            const raw = this.__element.getAttribute(this.__attribute);
            const index = this.__values.indexOf(raw);
            return index < 0 ? this.__fallback : index;
        }
        set baseVal(value) {
            value = Number(value) >>> 0;
            if (!value || value >= this.__values.length)
                throw new TypeError('Invalid SVG enumeration value');
            this.__element.setAttribute(this.__attribute, this.__values[value]);
        }
        get animVal() { return this.baseVal; }
    }
    class SVGNumber {
        constructor(token, value = 0, readOnly = false) {
            if (token !== svgFilterToken) throw new TypeError('Illegal constructor');
            this.__value = value;
            this.__list = null;
            this.__readOnly = readOnly;
        }
        get value() { return this.__value; }
        set value(value) {
            if (this.__readOnly)
                throw new DOMException('Animated SVG values are read-only', 'NoModificationAllowedError');
            value = Number(value);
            if (!Number.isFinite(value)) throw new TypeError('SVG numbers must be finite');
            this.__value = value;
            this.__list?.__serialize();
        }
    }
    class SVGNumberList {
        constructor(token, element, attribute, defaults, readOnly = false) {
            if (token !== svgFilterToken) throw new TypeError('Illegal constructor');
            this.__element = element;
            this.__attribute = attribute;
            this.__defaults = defaults;
            this.__readOnly = readOnly;
            this.__source = undefined;
            this.__items = [];
        }
        __sync() {
            const source = this.__element.getAttribute(this.__attribute);
            if (source === this.__source) return;
            const values = source === null ? this.__defaults() : parseSvgNumberList(source);
            for (const item of this.__items) item.__list = null;
            this.__items = values.map(value => {
                const item = new SVGNumber(svgFilterToken, value, this.__readOnly);
                item.__list = this;
                return item;
            });
            this.__source = source;
        }
        __serialize() {
            const source = this.__items.map(item => String(item.value)).join(' ');
            this.__element.setAttribute(this.__attribute, source);
            this.__source = source;
        }
        __writable() {
            if (this.__readOnly)
                throw new DOMException('Animated SVG lists are read-only', 'NoModificationAllowedError');
        }
        __accept(item) {
            this.__writable();
            if (!(item instanceof SVGNumber)) throw new TypeError('Expected SVGNumber');
            if (item.__readOnly) item = new SVGNumber(svgFilterToken, item.value);
            if (item.__list) {
                item.__list.__sync();
                const index = item.__list.__items.indexOf(item);
                if (index >= 0) {
                    item.__list.__items.splice(index, 1);
                    item.__list.__serialize();
                }
            }
            item.__list = this;
            return item;
        }
        get length() { this.__sync(); return this.__items.length; }
        get numberOfItems() { return this.length; }
        getItem(index) {
            this.__sync();
            index = Number(index) >>> 0;
            if (index >= this.__items.length)
                throw new DOMException('SVG list index is out of range', 'IndexSizeError');
            return this.__items[index];
        }
        clear() {
            this.__writable();
            this.__sync();
            for (const item of this.__items) item.__list = null;
            this.__items = [];
            this.__serialize();
        }
        initialize(item) {
            this.__writable();
            this.clear();
            return this.appendItem(item);
        }
        appendItem(item) {
            this.__writable();
            this.__sync();
            item = this.__accept(item);
            this.__items.push(item);
            this.__serialize();
            return item;
        }
        insertItemBefore(item, index) {
            this.__writable();
            this.__sync();
            index = Math.min(Number(index) >>> 0, this.__items.length);
            if (item instanceof SVGNumber && item.__list === this &&
                this.__items.indexOf(item) < index) index--;
            if (item instanceof SVGNumber && this.__items[index] === item) return item;
            item = this.__accept(item);
            this.__items.splice(index, 0, item);
            this.__serialize();
            return item;
        }
        replaceItem(item, index) {
            this.__writable();
            this.__sync();
            index = Number(index) >>> 0;
            if (index >= this.__items.length)
                throw new DOMException('SVG list index is out of range', 'IndexSizeError');
            if (this.__items[index] === item) return item;
            const oldIndex = item instanceof SVGNumber && item.__list === this
                ? this.__items.indexOf(item) : -1;
            item = this.__accept(item);
            if (oldIndex >= 0 && oldIndex < index) index--;
            this.__items[index].__list = null;
            this.__items[index] = item;
            this.__serialize();
            return item;
        }
        removeItem(index) {
            this.__writable();
            const item = this.getItem(index);
            this.__items.splice(Number(index) >>> 0, 1);
            item.__list = null;
            this.__serialize();
            return item;
        }
        [Symbol.iterator]() {
            this.__sync();
            return this.__items[Symbol.iterator]();
        }
    }
    class SVGAnimatedNumberList {
        constructor(token, element, attribute, defaults) {
            if (token !== svgFilterToken) throw new TypeError('Illegal constructor');
            this.__list = new SVGNumberList(token, element, attribute, defaults);
            this.__animated = new SVGNumberList(token, element, attribute, defaults, true);
        }
        get baseVal() { return this.__list; }
        get animVal() { return this.__animated; }
    }
    class SVGFEColorMatrixElement extends SVGElement {
        get in1() { return this.__in1 ||= new SVGAnimatedString(this, 'in'); }
        get type() {
            return this.__type ||= new SVGAnimatedEnumeration(
                svgFilterToken, this, 'type', colorMatrixTypes, 1);
        }
        get values() {
            return this.__values ||= new SVGAnimatedNumberList(
                svgFilterToken, this, 'values', () => []);
        }
    }
    SVGSVGElement.prototype.createSVGNumber = function() {
        return new SVGNumber(svgFilterToken);
    };
    for (const [name, value] of Object.entries({
        SVG_FECOLORMATRIX_TYPE_UNKNOWN: 0,
        SVG_FECOLORMATRIX_TYPE_MATRIX: 1,
        SVG_FECOLORMATRIX_TYPE_SATURATE: 2,
        SVG_FECOLORMATRIX_TYPE_HUEROTATE: 3,
        SVG_FECOLORMATRIX_TYPE_LUMINANCETOALPHA: 4
    })) {
        Object.defineProperty(SVGFEColorMatrixElement, name, { value, enumerable: true });
        Object.defineProperty(SVGFEColorMatrixElement.prototype, name, { value, enumerable: true });
    }
