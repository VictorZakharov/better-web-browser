    // HTML Standard: getHTML serializes a fragment, optionally including the
    // specified roots and serializable roots as declarative shadow templates.
    // Keep innerHTML's legacy path separate so its default remains light DOM.
    const getHtmlFragment = (node, options) => {
        if (options != null && typeof options !== 'object' && typeof options !== 'function')
            throw new TypeError('getHTML options must be a dictionary');
        const dictionary = Object(options);
        const serializable = !!dictionary.serializableShadowRoots;
        const suppliedRoots = dictionary.shadowRoots;
        const sequence = suppliedRoots === undefined ? [] : suppliedRoots;
        if (sequence == null || typeof sequence[Symbol.iterator] !== 'function')
            throw new TypeError('shadowRoots must be a sequence of ShadowRoot objects');
        const roots = [];
        for (const root of sequence) {
            if (!(root instanceof ShadowRoot))
                throw new TypeError('shadowRoots must contain only ShadowRoot objects');
            roots.push(nodeId(root));
        }
        return host('getHTML', nodeId(node), serializable, ...roots);
    };
    Object.defineProperty(Element.prototype, 'getHTML', {
        configurable: true, writable: true, enumerable: true,
        value: function getHTML(options = {}) {
            if (!(this instanceof Element)) throw new TypeError('Illegal invocation');
            return getHtmlFragment(this, options);
        }
    });
    Object.defineProperty(ShadowRoot.prototype, 'getHTML', {
        configurable: true, writable: true, enumerable: true,
        value: function getHTML(options = {}) {
            if (!(this instanceof ShadowRoot)) throw new TypeError('Illegal invocation');
            return getHtmlFragment(this, options);
        }
    });
