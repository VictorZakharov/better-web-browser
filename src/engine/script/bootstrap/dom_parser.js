    // HTML's detached parsing algorithms do not run scripts or use the live registry.
    // https://html.spec.whatwg.org/multipage/dynamic-markup-insertion.html#the-domparser-interface
    const domParsers = new WeakSet();
    class DOMParser {
        constructor() { domParsers.add(this); }
        parseFromString(input, type) {
            if (!domParsers.has(this)) throw new TypeError('Illegal invocation');
            if (arguments.length < 2) throw new TypeError('parseFromString requires two arguments');
            if (typeof input === 'symbol' || typeof type === 'symbol')
                throw new TypeError('Cannot convert a Symbol to a string');
            input = String(input);
            type = String(type);
            if (!['text/html', 'text/xml', 'application/xml', 'application/xhtml+xml', 'image/svg+xml'].includes(type))
                throw new TypeError('Unsupported DOMParser content type');
            return wrap(host('parseDocument', input, type));
        }
    }
    Object.defineProperty(DOMParser.prototype, Symbol.toStringTag, { value: 'DOMParser', configurable: true });
    // XHR uses the same native parser, but has its own MIME/URL/error-document contract.
    globalThis.__parseXhrDocument = (input, mime, url, allowHtml) => {
        const type = mime.split(';', 1)[0].trim().toLowerCase() || 'text/xml';
        if (type === 'text/html' ? !allowHtml : !(type === 'text/xml' || type === 'application/xml' || /^[^/\s]+\/[^/\s]+\+xml$/.test(type))) return null;
        return wrap(host('parseDocument', input, type, url));
    };
