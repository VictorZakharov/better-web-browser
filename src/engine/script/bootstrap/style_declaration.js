    const styleProxy = element => declarationProxy(new CSSStyleDeclaration(element));
    class CSSStyleDeclaration {
        constructor(element) { this.element = element; this.__source = null; this.__map = null; }
        _name(name) { name = String(name); return name.startsWith('--') ? name : name.toLowerCase(); }
        _map() {
            const source = host('attrGet', nodeId(this.element), 'style') || '';
            if (this.__source === source) return this.__map;
            const map = new Map();
            for (const [name, value, important] of host('cssDeclarationList', source, false))
                map.set(name, {value, priority:important ? 'important' : ''});
            this.__source = source; this.__map = map;
            return map;
        }
        _write(map) {
            setAttributeValueInternal(this.element, 'style', [...map]
                .map(([name, entry]) => name + ': ' + entry.value +
                    (entry.priority ? ' !important' : '')).join('; '));
        }
        get cssText() { return [...this._map()].map(([name, entry]) => name + ': ' +
            entry.value + (entry.priority ? ' !important' : '') + ';').join(' '); }
        set cssText(value) {
            const map = new Map(host('cssDeclarationList', String(value), false)
                .map(([name, text, important]) => [name,{value:text,priority:important ? 'important' : ''}]));
            this._write(map);
        }
        get length() { return this._map().size; }
        item(index) { return [...this._map().keys()][Number(index) >>> 0] || ''; }
        get parentRule() { return null; }
        getPropertyValue(name) { return declarationValue(this._map(), this._name(name)); }
        getPropertyPriority(name) { return declarationPriority(this._map(), this._name(name)); }
        setProperty(name, value, priority = '') {
            name = this._name(name); value = String(value); priority = String(priority).toLowerCase();
            if (!value) { this.removeProperty(name); return; }
            if (priority && priority !== 'important') return;
            const parsed = host('cssDeclarationValue', name, value);
            if (parsed === null) return;
            const map = new Map(this._map());
            if (!setDeclarationValue(map, name, parsed, priority)) return;
            this._write(map);
        }
        removeProperty(name) {
            const map = new Map(this._map());
            name = this._name(name);
            const old = declarationValue(map, name);
            if (!removeDeclarationValue(map, name)) return '';
            this._write(map);
            return old;
        }
    }
    const fontLonghands=Object.freeze(host('cssFontLonghands'));
    const declarationValue = (map, name) => {
        if (name === 'font') {
            const pending=map.get(name);
            if (pending) return pending.value;
            const entries=fontLonghands.map(property=>map.get(property));
            if (entries.some(entry=>!entry || entry.priority !== entries[0].priority)) return '';
            return host('cssFontValue', ...entries.map(entry=>entry.value));
        }
        if (name !== 'font-variant') return map.get(name)?.value || '';
        const ligatures=map.get('font-variant-ligatures'), numeric=map.get('font-variant-numeric');
        if (!ligatures || !numeric || ligatures.priority !== numeric.priority) return '';
        return host('cssFontVariantValue', ligatures.value, numeric.value);
    };
    const setDeclarationValue = (map, name, value, priority) => {
        const expanded = host('cssDeclarationExpansion', name, value);
        if (expanded.every(([property, text])=>map.get(property)?.value===text &&
            map.get(property)?.priority===priority) &&
            (expanded.length===1 || !map.has(name))) return false;
        // Remove the old shorthand as well (e.g. a pending var() declaration).
        if (expanded.length > 1) {
            map.delete(name);
            for (const [property] of expanded) map.delete(property);
        }
        for (const [property, text] of expanded) map.set(property, {value:text, priority});
        return true;
    };
    const declarationPriority = (map, name) => {
        if (name === 'font') {
            if (map.has(name)) return map.get(name).priority;
            return declarationValue(map, name) ? map.get(fontLonghands[0]).priority : '';
        }
        if (name !== 'font-variant') return map.get(name)?.priority || '';
        return declarationValue(map, name) ? map.get('font-variant-ligatures').priority : '';
    };
    const removeDeclarationValue = (map, name) => {
        let changed=map.delete(name);
        if (name === 'font-variant') {
            changed=map.delete('font-variant-ligatures') || changed;
            changed=map.delete('font-variant-numeric') || changed;
        }
        if (name === 'font') {
            for (const property of fontLonghands) changed=map.delete(property) || changed;
        }
        return changed;
    };
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
            if (typeof property === 'string' && /^(0|[1-9][0-9]*)$/.test(property))
                return Number(property) < target.length ? target.item(Number(property)) : undefined;
            if (Reflect.has(target, property)) {
                const value = Reflect.get(target, property, receiver);
                return typeof value === 'function' ? value.bind(target) : value;
            }
            const name = styleAttributeName(property);
            return name === null ? undefined : target.getPropertyValue(name);
        },
        has(target, property) {
            if (typeof property === 'string' && /^(0|[1-9][0-9]*)$/.test(property))
                return Number(property) < target.length;
            return Reflect.has(target, property) || styleAttributeName(property) !== null;
        },
        set(target, property, value, receiver) {
            if (typeof property === 'string' && /^(0|[1-9][0-9]*)$/.test(property)) return false;
            if (!Reflect.has(target, property)) {
                const name = styleAttributeName(property);
                if (name !== null) { target.setProperty(name, value); return true; }
            }
            return Reflect.set(target, property, value, receiver);
        }
    });
