    // LinkStyle and Document.styleSheets expose the sheets already fetched by the page loader.
    // CSSOM requires a live, same-object list and blocks rule access for non-origin-clean sheets.
    const ownerStyleSheets = new WeakMap();
    const documentStyleSheetLists = new WeakMap();

    function isCssStyleElement(owner) {
        return owner instanceof HTMLStyleElement &&
            (!owner.type || owner.type.toLowerCase() === 'text/css');
    }

    function isStyleSheetLink(owner) {
        return owner instanceof HTMLLinkElement &&
            owner.rel.toLowerCase().split(/\s+/).includes('stylesheet') &&
            (!owner.type || owner.type.toLowerCase() === 'text/css');
    }

    function associatedStyleSheet(owner) {
        if (!owner?.isConnected || (!isCssStyleElement(owner) && !isStyleSheetLink(owner)))
            return null;
        const href = isStyleSheetLink(owner) ? owner.href : null;
        const metadata = [href, owner.getAttribute('title'), owner.media, owner.disabled].join('\u0000');
        let record = ownerStyleSheets.get(owner);
        if (href !== null && record?.metadata === metadata) return record.sheet;
        const source = href === null ? owner.textContent : host('stylesheetSource', href);
        if (source === null) return null;
        const signature = href === null ? metadata + '\u0000' + source : metadata;
        if (record?.signature === signature) return record.sheet;
        if (!record) {
            record = { sheet: new CSSStyleSheet(), metadata: null, signature: null };
            ownerStyleSheets.set(owner, record);
        }
        record.sheet.__setOwner(
            owner,
            href,
            owner.getAttribute('title') || null,
            owner.media,
            href === null || host('stylesheetSameOrigin', href),
            source
        );
        record.metadata = metadata;
        record.signature = signature;
        return record.sheet;
    }

    const styleSheetListToken = {};
    class StyleSheetList {
        constructor(token, ownerDocument) {
            if (token !== styleSheetListToken) throw new TypeError('Illegal constructor');
            this.__document = ownerDocument;
        }
        __snapshot() {
            return Array.from(this.__document.querySelectorAll('style, link'))
                .map(associatedStyleSheet)
                .filter(sheet => sheet !== null);
        }
        get length() { return this.__snapshot().length; }
        item(index) { return this.__snapshot()[Number(index)] || null; }
        [Symbol.iterator]() { return this.__snapshot()[Symbol.iterator](); }
    }

    function styleSheetListFor(ownerDocument) {
        let list = documentStyleSheetLists.get(ownerDocument);
        if (list) return list;
        const target = new StyleSheetList(styleSheetListToken, ownerDocument);
        list = new Proxy(target, {
            get(target, property, receiver) {
                if (cssIndex(property)) return target.item(property) || undefined;
                const value = Reflect.get(target, property, receiver);
                return typeof value === 'function' ? value.bind(target) : value;
            },
            ownKeys(target) {
                return [...Array(target.length).keys()].map(String);
            },
            getOwnPropertyDescriptor(target, property) {
                if (cssIndex(property) && Number(property) < target.length)
                    return { configurable: true, enumerable: true, value: target.item(property) };
            }
        });
        documentStyleSheetLists.set(ownerDocument, list);
        return list;
    }

    Object.defineProperty(Document.prototype, 'styleSheets', {
        configurable: true,
        enumerable: true,
        get() { return styleSheetListFor(this); }
    });
    Object.defineProperty(HTMLStyleElement.prototype, 'sheet', {
        configurable: true,
        enumerable: true,
        get() { return associatedStyleSheet(this); }
    });
    Object.defineProperty(HTMLLinkElement.prototype, 'sheet', {
        configurable: true,
        enumerable: true,
        get() { return associatedStyleSheet(this); }
    });
    Object.defineProperty(StyleSheetList.prototype, Symbol.toStringTag,
        { value: 'StyleSheetList', configurable: true });
    windowObject.StyleSheetList = StyleSheetList;
