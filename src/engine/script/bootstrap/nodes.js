    class Node extends EventTarget {
        constructor(id) {
            super();
            this.__id = id;
        }
        get nodeType() { return host('nodeType', this.__id); }
        get nodeName() {
            return this.nodeType === 1 ? host('tagName', this.__id) :
                this.nodeType === 9 ? '#document' :
                this.nodeType === 11 ? '#document-fragment' :
                this.nodeType === 3 ? '#text' : '#comment';
        }
        get ownerDocument() { return this.nodeType === 9 ? null : document; }
        get parentNode() { return wrap(host('parent', this.__id)); }
        get parentElement() { const parent = this.parentNode; return parent?.nodeType === 1 ? parent : null; }
        get firstChild() { return wrap(host('firstChild', this.__id)); }
        get lastChild() { return wrap(host('lastChild', this.__id)); }
        get nextSibling() { return wrap(host('nextSibling', this.__id)); }
        get previousSibling() { return wrap(host('previousSibling', this.__id)); }
        get childNodes() { return list(host('children', this.__id)); }
        get children() { return list(host('elementChildren', this.__id)); }
        get firstElementChild() { return this.children[0] || null; }
        get lastElementChild() { const children = this.children; return children[children.length - 1] || null; }
        get childElementCount() { return this.children.length; }
        get textContent() { return host('textGet', this.__id); }
        set textContent(value) { host('textSet', this.__id, value == null ? '' : String(value)); }
        get isConnected() {
            let node = this;
            while (node) {
                if (node.nodeType === 9) return true;
                node = node.parentNode;
            }
            return false;
        }
        appendChild(child) {
            if (!(child instanceof Node)) throw new TypeError('appendChild requires a Node');
            return wrap(host('appendChild', this.__id, child.__id));
        }
        append(...items) {
            for (const item of items) this.appendChild(item instanceof Node ? item : document.createTextNode(String(item)));
        }
        prepend(...items) {
            let reference = this.firstChild;
            for (const item of items) {
                const node = item instanceof Node ? item : document.createTextNode(String(item));
                this.insertBefore(node, reference);
                if (!reference) reference = node.nextSibling;
            }
        }
        insertBefore(child, reference) {
            if (!(child instanceof Node)) throw new TypeError('insertBefore requires a Node');
            if (reference != null && !(reference instanceof Node)) throw new TypeError('reference must be a Node');
            return wrap(host('insertBefore', this.__id, child.__id, reference?.__id || 0));
        }
        removeChild(child) {
            if (!(child instanceof Node) || !host('removeChild', this.__id, child.__id)) throw new Error('node is not a child');
            return child;
        }
        remove() { host('remove', this.__id); }
        contains(other) {
            for (let node = other; node; node = node.parentNode) if (node === this) return true;
            return false;
        }
        hasChildNodes() { return !!this.firstChild; }
        querySelector(selector) { return wrap(host('query', this.__id, String(selector))); }
        querySelectorAll(selector) { return list(host('queryAll', this.__id, String(selector))); }
    }

    class Text extends Node {
        get data() { return this.textContent; }
        set data(value) { this.textContent = value; }
        get nodeValue() { return this.data; }
        set nodeValue(value) { this.data = value == null ? '' : String(value); }
        get length() { return this.data.length; }
    }

    class Comment extends Text {}
    class DocumentFragment extends Node {}

    class DOMTokenList {
        constructor(element, attribute) { this.element = element; this.attribute = attribute; }
        _tokens() { return (this.element.getAttribute(this.attribute) || '').split(/\s+/).filter(Boolean); }
        _set(tokens) { this.element.setAttribute(this.attribute, [...new Set(tokens)].join(' ')); }
        contains(token) { return this._tokens().includes(String(token)); }
        add(...tokens) { this._set(this._tokens().concat(tokens.map(String))); }
        remove(...tokens) { const remove = new Set(tokens.map(String)); this._set(this._tokens().filter(token => !remove.has(token))); }
        toggle(token, force) {
            token = String(token);
            const present = this.contains(token);
            if (force === true || (!present && force !== false)) { this.add(token); return true; }
            if (present) this.remove(token);
            return false;
        }
        replace(oldToken, newToken) {
            const tokens = this._tokens();
            const index = tokens.indexOf(String(oldToken));
            if (index < 0) return false;
            tokens[index] = String(newToken);
            this._set(tokens);
            return true;
        }
        get value() { return this.element.getAttribute(this.attribute) || ''; }
        set value(value) { this.element.setAttribute(this.attribute, value); }
        get length() { return this._tokens().length; }
        item(index) { return this._tokens()[index] || null; }
        [Symbol.iterator]() { return this._tokens()[Symbol.iterator](); }
        toString() { return this.value; }
    }

    class CSSStyleDeclaration {
        constructor(element) { this.element = element; }
        _map() {
            const map = new Map();
            for (const declaration of (this.element.getAttribute('style') || '').split(';')) {
                const split = declaration.indexOf(':');
                if (split > 0) map.set(declaration.slice(0, split).trim().toLowerCase(), declaration.slice(split + 1).trim());
            }
            return map;
        }
        _write(map) { this.element.setAttribute('style', [...map].map(([name, value]) => name + ': ' + value).join('; ')); }
        get cssText() { return this.element.getAttribute('style') || ''; }
        set cssText(value) { this.element.setAttribute('style', String(value)); }
        getPropertyValue(name) { return this._map().get(String(name).toLowerCase()) || ''; }
        setProperty(name, value, priority = '') {
            const map = this._map();
            map.set(String(name).toLowerCase(), String(value) + (priority ? ' !' + priority : ''));
            this._write(map);
        }
        removeProperty(name) {
            const map = this._map();
            const old = map.get(String(name).toLowerCase()) || '';
            map.delete(String(name).toLowerCase());
            this._write(map);
            return old;
        }
    }
    const styleProxy = element => new Proxy(new CSSStyleDeclaration(element), {
        get(target, property) {
            if (property in target) {
                const value = target[property];
                return typeof value === 'function' ? value.bind(target) : value;
            }
            return target.getPropertyValue(String(property).replace(/[A-Z]/g, match => '-' + match.toLowerCase()));
        },
        set(target, property, value) {
            if (property === 'cssText') target.cssText = value;
            else target.setProperty(String(property).replace(/[A-Z]/g, match => '-' + match.toLowerCase()), value);
            return true;
        }
    });
