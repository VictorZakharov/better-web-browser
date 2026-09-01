    // CSSOM View geometry is kept separate from the core Element bindings because every read may
    // synchronously flush style and layout. The host owns that flush and caches its geometry until
    // the DOM mutation version changes.
    // https://drafts.csswg.org/cssom-view/#extensions-to-the-element-interface
    function layoutRect(element) {
        const value = host('layoutRect', element.__id);
        const [x = 0, y = 0, width = 0, height = 0] = Array.isArray(value) ? value : [];
        return { x, y, width, height, hasBox: Array.isArray(value) };
    }

    function isViewportElement(element) {
        return element.ownerDocument === document &&
            ((document.compatMode !== 'BackCompat' && element === document.documentElement) ||
             (document.compatMode === 'BackCompat' && element === document.body));
    }

    function computedBorderWidth(element, side) {
        return Number.parseFloat(getComputedStyle(element)[`border${side}Width`]) || 0;
    }

    function computedPaddingWidth(element, side) {
        return Number.parseFloat(getComputedStyle(element)[`padding${side}`]) || 0;
    }

    function clientWidth(element) {
        if (isViewportElement(element)) return layoutViewportWidth;
        const rect = layoutRect(element);
        if (!rect.hasBox) return 0;
        if (element.localName === 'table') return rect.width;
        let width = rect.width - computedBorderWidth(element, 'Left') -
            computedBorderWidth(element, 'Right');
        if (element.localName === 'input') {
            width -= computedPaddingWidth(element, 'Left') + computedPaddingWidth(element, 'Right');
        }
        return Math.max(0, width);
    }

    function clientHeight(element) {
        if (isViewportElement(element)) return layoutViewportHeight;
        const rect = layoutRect(element);
        if (!rect.hasBox) return 0;
        if (element.localName === 'table') return rect.height;
        return Math.max(0, rect.height - computedBorderWidth(element, 'Top') -
            computedBorderWidth(element, 'Bottom'));
    }

    Object.defineProperties(Element.prototype, {
        __layoutRect: {
            configurable: true,
            value() { return layoutRect(this); }
        },
        clientWidth: {
            configurable: true,
            get() { return Math.round(clientWidth(this)); }
        },
        clientHeight: {
            configurable: true,
            get() { return Math.round(clientHeight(this)); }
        },
        clientLeft: {
            configurable: true,
            get() {
                if (isViewportElement(this) || this.localName === 'table' ||
                    !layoutRect(this).hasBox) return 0;
                const padding = this.localName === 'input' ? computedPaddingWidth(this, 'Left') : 0;
                return Math.round(computedBorderWidth(this, 'Left') + padding);
            }
        },
        clientTop: {
            configurable: true,
            get() {
                if (isViewportElement(this) || this.localName === 'table' ||
                    !layoutRect(this).hasBox) return 0;
                return Math.round(computedBorderWidth(this, 'Top'));
            }
        },
        getBoundingClientRect: {
            configurable: true,
            value() {
                const { x, y, width, height } = layoutRect(this);
                const value = { x, y, top: y, left: x, right: x + width, bottom: y + height,
                    width, height };
                return { ...value, toJSON() { return { ...value }; } };
            }
        }
    });

    // CSSOM View defines offset geometry on HTMLElement, not Element. Foreign-content elements
    // such as SVG therefore do not expose these properties through their prototype chain.
    // https://drafts.csswg.org/cssom-view/#extensions-to-the-htmlelement-interface
    Object.defineProperties(HTMLElement.prototype, {
        offsetWidth: {
            configurable: true,
            get() { return Math.round(layoutRect(this).width); }
        },
        offsetHeight: {
            configurable: true,
            get() { return Math.round(layoutRect(this).height); }
        },
        offsetParent: {
            configurable: true,
            get() { return wrap(host('offsetParent', this.__id)); }
        },
        offsetLeft: {
            configurable: true,
            get() {
                const rect = layoutRect(this);
                const parent = this.offsetParent;
                if (!parent) return Math.round(rect.x);
                const parentRect = layoutRect(parent);
                const border = computedBorderWidth(parent, 'Left');
                return Math.round(rect.x - parentRect.x - border);
            }
        },
        offsetTop: {
            configurable: true,
            get() {
                const rect = layoutRect(this);
                const parent = this.offsetParent;
                if (!parent) return Math.round(rect.y);
                const parentRect = layoutRect(parent);
                const border = computedBorderWidth(parent, 'Top');
                return Math.round(rect.y - parentRect.y - border);
            }
        }
    });
