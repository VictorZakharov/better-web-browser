    const styleProxy = element => declarationProxy(new CSSStyleDeclaration(element));
    class CSSStyleDeclaration {
        constructor(element) { this.element = element; }
        _name(name) { name = String(name); return name.startsWith('--') ? name : name.toLowerCase(); }
        _map() {
            const map = new Map();
            for (const declaration of this.cssText.split(';')) {
                const split = declaration.indexOf(':');
                if (split > 0) {
                    const name = declaration.slice(0, split).trim();
                    map.set(this._name(name), declaration.slice(split + 1).trim());
                }
            }
            return map;
        }
        _write(map) {
            setAttributeValueInternal(this.element, 'style', [...map]
                .map(([name, value]) => name + ': ' + value).join('; '));
        }
        get cssText() { return host('attrGet', nodeId(this.element), 'style') || ''; }
        set cssText(value) { setAttributeValueInternal(this.element, 'style', String(value)); }
        getPropertyValue(name) { return this._map().get(this._name(name)) || ''; }
        setProperty(name, value, priority = '') {
            const map = this._map();
            map.set(this._name(name), String(value) + (priority ? ' !' + priority : ''));
            this._write(map);
        }
        removeProperty(name) {
            const map = this._map();
            name = this._name(name);
            const old = map.get(name) || '';
            map.delete(name);
            this._write(map);
            return old;
        }
    }
    // CSSOM exposes named attributes only for supported CSS properties. Unknown
    // names/symbols retain ordinary JavaScript lookup and expando semantics.
    // https://drafts.csswg.org/cssom/#the-cssstyledeclaration-interface
    const supportedStyleNames = new Map();
    const styleAttributeName = property => {
        if (typeof property !== 'string' || property.startsWith('--')) return null;
        if (!supportedStyleNames.has(property)) {
            const name = property === 'cssFloat' ? 'float' :
                property.replace(/^webkit(?=[A-Z])/, 'Webkit')
                    .replace(/[A-Z]/g, match => '-' + match.toLowerCase());
            supportedStyleNames.set(property, host('cssPropertySupported', name) ? name : null);
        }
        return supportedStyleNames.get(property);
    };
    const declarationProxy = declaration => new Proxy(declaration, {
        get(target, property, receiver) {
            if (Reflect.has(target, property)) {
                const value = Reflect.get(target, property, receiver);
                return typeof value === 'function' ? value.bind(target) : value;
            }
            const name = styleAttributeName(property);
            return name === null ? undefined : target.getPropertyValue(name);
        },
        has(target, property) {
            return Reflect.has(target, property) || styleAttributeName(property) !== null;
        },
        set(target, property, value, receiver) {
            if (!Reflect.has(target, property)) {
                const name = styleAttributeName(property);
                if (name !== null) { target.setProperty(name, value); return true; }
            }
            return Reflect.set(target, property, value, receiver);
        }
    });
