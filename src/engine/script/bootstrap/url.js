    const formUrlDecode = value => decodeURIComponent(String(value).replace(/\+/g, ' '));
    const formUrlEncode = value => encodeURIComponent(String(value))
        .replace(/[!'()~]/g, character => '%' + character.charCodeAt(0).toString(16).toUpperCase())
        .replace(/%20/g, '+');
    class URLSearchParams {
        constructor(init = '', update = null) {
            this._entries = [];
            this._update = typeof update === 'function' ? update : null;
            this._replace(init);
        }
        _replace(init) {
            this._entries = [];
            if (typeof init === 'string') {
                const source = init.replace(/^\?/, '');
                if (source) for (const part of source.split('&')) {
                    const split = part.indexOf('=');
                    const key = split < 0 ? part : part.slice(0, split);
                    const value = split < 0 ? '' : part.slice(split + 1);
                    this._entries.push([formUrlDecode(key), formUrlDecode(value)]);
                }
            } else if (init != null && typeof init[Symbol.iterator] === 'function') {
                for (const pair of init) {
                    const values = [...pair];
                    if (values.length !== 2) throw new TypeError('URLSearchParams pairs must contain two items');
                    this._entries.push([String(values[0]), String(values[1])]);
                }
            } else if (init != null) {
                for (const key of Object.keys(Object(init))) this._entries.push([String(key), String(init[key])]);
            }
        }
        _changed() { this._update?.(this.toString()); }
        get size() { return this._entries.length; }
        append(key, value) {
            this._entries.push([String(key), String(value)]);
            this._changed();
        }
        set(key, value) {
            key = String(key);
            value = String(value);
            let replaced = false;
            this._entries = this._entries.filter(entry => {
                if (entry[0] !== key) return true;
                if (replaced) return false;
                entry[1] = value;
                replaced = true;
                return true;
            });
            if (!replaced) this._entries.push([key, value]);
            this._changed();
        }
        get(key) { return this._entries.find(entry => entry[0] === String(key))?.[1] ?? null; }
        getAll(key) { return this._entries.filter(entry => entry[0] === String(key)).map(entry => entry[1]); }
        has(key, value = undefined) {
            key = String(key);
            return value === undefined
                ? this._entries.some(entry => entry[0] === key)
                : this._entries.some(entry => entry[0] === key && entry[1] === String(value));
        }
        delete(key, value = undefined) {
            key = String(key);
            this._entries = value === undefined
                ? this._entries.filter(entry => entry[0] !== key)
                : this._entries.filter(entry => entry[0] !== key || entry[1] !== String(value));
            this._changed();
        }
        sort() {
            this._entries = this._entries
                .map((entry, index) => ({ entry, index }))
                .sort((left, right) => left.entry[0] < right.entry[0] ? -1
                    : left.entry[0] > right.entry[0] ? 1 : left.index - right.index)
                .map(item => item.entry);
            this._changed();
        }
        forEach(callback, thisArg = undefined) {
            for (const [key, value] of this._entries) callback.call(thisArg, value, key, this);
        }
        toString() {
            return this._entries.map(([key, value]) => formUrlEncode(key) + '=' + formUrlEncode(value)).join('&');
        }
        entries() { return this._entries[Symbol.iterator](); }
        keys() { return this._entries.map(entry => entry[0])[Symbol.iterator](); }
        values() { return this._entries.map(entry => entry[1])[Symbol.iterator](); }
        [Symbol.iterator]() { return this.entries(); }
    }
    windowObject.URLSearchParams = URLSearchParams;

    const missingUrlValue = {};
    windowObject.URL = class URL {
        constructor(value = missingUrlValue, base = currentUrl) {
            if (value === missingUrlValue) throw new TypeError('URL requires an input');
            this._href = host('strictResolveUrl', String(value), String(base));
            this._searchParams = new URLSearchParams(this.search, value => this._setSearchFromParams(value));
        }
        static canParse(value, base = currentUrl) {
            try { host('strictResolveUrl', String(value), String(base)); return true; }
            catch (_) { return false; }
        }
        static parse(value, base = currentUrl) {
            try { return new URL(value, base); } catch (_) { return null; }
        }
        _parts() { return parseUrl(this._href); }
        _set(component, value, updateParams = false) {
            try { this._href = host('setWebUrlComponent', this._href, component, String(value)); }
            catch (_) { return; }
            if (updateParams) this._searchParams._replace(this.search);
        }
        _setSearchFromParams(value) { this._set('search', value); }
        toString() { return this._href; }
        toJSON() { return this._href; }
        get href() { return this._href; }
        set href(value) {
            this._href = host('setWebUrlComponent', this._href, 'href', String(value));
            this._searchParams._replace(this.search);
        }
        get protocol() { return this._parts().protocol; }
        set protocol(value) { this._set('protocol', value); }
        get username() { return this._parts().username; }
        set username(value) { this._set('username', value); }
        get password() { return this._parts().password; }
        set password(value) { this._set('password', value); }
        get host() { return this._parts().host; }
        set host(value) { this._set('host', value); }
        get hostname() { return this._parts().hostname; }
        set hostname(value) { this._set('hostname', value); }
        get port() { return this._parts().port; }
        set port(value) { this._set('port', value); }
        get pathname() { return this._parts().pathname; }
        set pathname(value) { this._set('pathname', value); }
        get search() { return this._parts().search; }
        set search(value) { this._set('search', value, true); }
        get hash() { return this._parts().hash; }
        set hash(value) { this._set('hash', value); }
        get origin() { return this._parts().origin; }
        get searchParams() { return this._searchParams; }
    };
