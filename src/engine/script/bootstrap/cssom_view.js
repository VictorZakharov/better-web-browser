    // CSSOM View geometry is kept separate from the core Element bindings because every read may
    // synchronously flush style and layout. The host owns that flush and caches its geometry until
    // the DOM mutation version changes.
    // https://drafts.csswg.org/cssom-view/#extensions-to-the-element-interface
    let viewportScrollX = 0;
    let viewportScrollY = 0;

    function layoutRect(element) {
        const value = host('layoutRect', nodeId(element));
        const [x = 0, y = 0, width = 0, height = 0] = Array.isArray(value) ? value : [];
        return { x, y, width, height, hasBox: Array.isArray(value) };
    }

    function isViewportElement(element) {
        return element.ownerDocument === document &&
            ((document.compatMode !== 'BackCompat' && element === document.documentElement) ||
             (document.compatMode === 'BackCompat' && element === document.body));
    }

    function effectiveZoom(element) {
        return host('effectiveZoom', nodeId(element)) || 1;
    }

    function computedBorderWidth(element, side) {
        return Number.parseFloat(getComputedStyle(element)[`border${side}Width`]) || 0;
    }

    function computedPaddingWidth(element, side) {
        return Number.parseFloat(getComputedStyle(element)[`padding${side}`]) || 0;
    }

    // The anonymous table wrapper owns the client box, regardless of element tag.
    // Its border is on the inner table box, not on this wrapper (CSS 2.2 section 17.4).
    function hasTableWrapper(element) {
        const display = getComputedStyle(element).display;
        return display === 'table' || display === 'inline-table';
    }

    function clientWidth(element) {
        if (isViewportElement(element)) return layoutViewportWidth;
        const scroll = host('elementScroll', nodeId(element));
        if (Array.isArray(scroll)) return scroll[4];
        const rect = layoutRect(element);
        if (!rect.hasBox) return 0;
        const widthWithoutZoom = rect.width / effectiveZoom(element);
        if (hasTableWrapper(element)) return widthWithoutZoom;
        let width = widthWithoutZoom - computedBorderWidth(element, 'Left') -
            computedBorderWidth(element, 'Right');
        if (element.localName === 'input') {
            width -= computedPaddingWidth(element, 'Left') + computedPaddingWidth(element, 'Right');
        }
        return Math.max(0, width);
    }

    function clientHeight(element) {
        if (isViewportElement(element)) return layoutViewportHeight;
        const scroll = host('elementScroll', nodeId(element));
        if (Array.isArray(scroll)) return scroll[5];
        const rect = layoutRect(element);
        if (!rect.hasBox) return 0;
        const heightWithoutZoom = rect.height / effectiveZoom(element);
        if (hasTableWrapper(element)) return heightWithoutZoom;
        return Math.max(0, heightWithoutZoom - computedBorderWidth(element, 'Top') -
            computedBorderWidth(element, 'Bottom'));
    }

    Object.defineProperties(Element.prototype, {
        currentCSSZoom: {
            configurable: true, enumerable: true,
            get() { return layoutRect(this).hasBox ? effectiveZoom(this) : 1; }
        },
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
                if (isViewportElement(this) || hasTableWrapper(this) ||
                    !layoutRect(this).hasBox) return 0;
                const padding = this.localName === 'input' ? computedPaddingWidth(this, 'Left') : 0;
                return Math.round(computedBorderWidth(this, 'Left') + padding);
            }
        },
        clientTop: {
            configurable: true,
            get() {
                if (isViewportElement(this) || hasTableWrapper(this) ||
                    !layoutRect(this).hasBox) return 0;
                return Math.round(computedBorderWidth(this, 'Top'));
            }
        },
    });

    // CSSOM View defines offset geometry on HTMLElement, not Element. Foreign-content elements
    // such as SVG therefore do not expose these properties through their prototype chain.
    // https://drafts.csswg.org/cssom-view/#extensions-to-the-htmlelement-interface
    Object.defineProperties(HTMLElement.prototype, {
        offsetWidth: {
            configurable: true,
            get() { return Math.round(layoutRect(this).width / effectiveZoom(this)); }
        },
        offsetHeight: {
            configurable: true,
            get() { return Math.round(layoutRect(this).height / effectiveZoom(this)); }
        },
        offsetParent: {
            configurable: true,
            get() { return wrap(host('offsetParent', nodeId(this))); }
        },
        offsetLeft: {
            configurable: true,
            get() {
                const rect = layoutRect(this);
                const parent = this.offsetParent;
                const zoom = effectiveZoom(this);
                if (!parent) return Math.round(rect.x / zoom);
                const parentRect = layoutRect(parent);
                const border = computedBorderWidth(parent, 'Left');
                return Math.round((rect.x - parentRect.x - border * effectiveZoom(parent)) / zoom);
            }
        },
        offsetTop: {
            configurable: true,
            get() {
                const rect = layoutRect(this);
                const parent = this.offsetParent;
                const zoom = effectiveZoom(this);
                if (!parent) return Math.round(rect.y / zoom);
                const parentRect = layoutRect(parent);
                const border = computedBorderWidth(parent, 'Top');
                return Math.round((rect.y - parentRect.y - border * effectiveZoom(parent)) / zoom);
            }
        }
    });
