    // HTML's HyperlinkElementUtils reparses the reflected href for every read. Keeping the
    // serialized URL in the attribute also makes component setters observable to markup.
    const hyperlinkUrl = element => {
        const value = element.getAttribute('href');
        if (value == null) return null;
        try { return host('strictResolveUrl', value); }
        catch (_) { return null; }
    };
    const hyperlinkParts = element => {
        const url = hyperlinkUrl(element);
        return url == null ? null : JSON.parse(host('parseWebUrl', url));
    };
    const setHyperlinkPart = (element, component, value) => {
        const url = hyperlinkUrl(element);
        if (url == null) return;
        try { element.setAttribute('href', host('setWebUrlComponent', url, component, String(value))); }
        catch (_) {}
    };
    const hyperlinkComponent = (component, missingValue) => ({
        configurable: true,
        enumerable: true,
        get() { return hyperlinkParts(this)?.[component] ?? missingValue; },
        set(value) { setHyperlinkPart(this, component, value); }
    });
    Object.defineProperties(HTMLAnchorElement.prototype, {
        href: {
            configurable: true,
            enumerable: true,
            get() {
                const url = hyperlinkUrl(this);
                return url ?? (this.getAttribute('href') || '');
            },
            set(value) { this.setAttribute('href', String(value)); }
        },
        origin: {
            configurable: true,
            enumerable: true,
            get() { return hyperlinkParts(this)?.origin ?? ''; }
        },
        protocol: hyperlinkComponent('protocol', ':'),
        username: hyperlinkComponent('username', ''),
        password: hyperlinkComponent('password', ''),
        host: hyperlinkComponent('host', ''),
        hostname: hyperlinkComponent('hostname', ''),
        port: hyperlinkComponent('port', ''),
        pathname: hyperlinkComponent('pathname', ''),
        search: hyperlinkComponent('search', ''),
        hash: hyperlinkComponent('hash', ''),
        toString: {
            configurable: true,
            writable: true,
            value() { return this.href; }
        }
    });
