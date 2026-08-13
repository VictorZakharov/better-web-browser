
(() => {
    'use strict';
    const host = (...args) => __hostCall(...args);
    const isolatedIframeWindow = globalThis.__iframeWindow;
    delete globalThis.__iframeWindow;
    if (typeof String.prototype.substr !== 'function') {
        Object.defineProperty(String.prototype, 'substr', {
            configurable: true,
            writable: true,
            value(start, length) {
                const string = String(this);
                const size = string.length;
                let from = Number(start) || 0;
                from = from < 0 ? Math.max(size + Math.ceil(from), 0) : Math.min(Math.floor(from), size);
                if (length === undefined) return string.slice(from);
                let count = Number(length);
                if (Number.isNaN(count) || count <= 0) return '';
                if (count !== Infinity) count = Math.floor(count);
                return string.slice(from, Math.min(from + count, size));
            }
        });
    }
    const cache = new Map();
    const list = value => {
        if (!value) return [];
        const result = value.split(',').filter(Boolean).map(id => wrap(Number(id)));
        result.item = index => result[index] || null;
        return result;
    };

    class Event {
        constructor(type, init = {}) {
            this.type = String(type);
            this.bubbles = !!init.bubbles;
            this.cancelable = !!init.cancelable;
            this.defaultPrevented = false;
            this.target = null;
            this.currentTarget = null;
            this.timeStamp = Date.now();
        }
        preventDefault() { if (this.cancelable) this.defaultPrevented = true; }
        stopPropagation() { this.__stopped = true; }
        stopImmediatePropagation() { this.__stopped = this.__immediate = true; }
    }
    class CustomEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            this.detail = init.detail === undefined ? null : init.detail;
        }
    }
    class MessageEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            this.data = init.data === undefined ? null : init.data;
            this.origin = init.origin === undefined ? '' : String(init.origin);
            this.lastEventId = init.lastEventId === undefined ? '' : String(init.lastEventId);
            this.source = init.source === undefined ? null : init.source;
            this.ports = Object.freeze([...(init.ports || [])]);
        }
        initMessageEvent(type, bubbles = false, cancelable = false, data = null, origin = '', lastEventId = '', source = null, ports = []) {
            this.type = String(type);
            this.bubbles = !!bubbles;
            this.cancelable = !!cancelable;
            this.data = data;
            this.origin = String(origin);
            this.lastEventId = String(lastEventId);
            this.source = source;
            this.ports = Object.freeze([...ports]);
        }
    }
    class ToggleEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            this.oldState = init.oldState === undefined ? '' : String(init.oldState);
            this.newState = init.newState === undefined ? '' : String(init.newState);
            this.source = init.source === undefined ? null : init.source;
        }
    }
    class DOMException extends Error {
        constructor(message = '', name = 'Error') {
            super(String(message));
            this.name = String(name);
            this.code = 0;
        }
    }
    const listenerStore = new WeakMap();
    class EventTarget {
        addEventListener(type, callback) {
            if (typeof callback !== 'function' && !(callback && typeof callback.handleEvent === 'function')) return;
            let listeners = listenerStore.get(this);
            if (!listeners) listenerStore.set(this, listeners = new Map());
            const bucket = listeners.get(String(type)) || [];
            if (!bucket.includes(callback)) bucket.push(callback);
            listeners.set(String(type), bucket);
        }
        removeEventListener(type, callback) {
            const bucket = listenerStore.get(this)?.get(String(type));
            if (!bucket) return;
            const index = bucket.indexOf(callback);
            if (index >= 0) bucket.splice(index, 1);
        }
        dispatchEvent(event) {
            if (!(event instanceof Event)) event = new Event(String(event));
            const dispatchTarget = this.__eventTargetProxy || this;
            event.target ||= dispatchTarget;
            event.currentTarget = dispatchTarget;
            const bucket = listenerStore.get(this)?.get(event.type) || [];
            for (const callback of [...bucket]) {
                if (typeof callback === 'function') callback.call(dispatchTarget, event);
                else callback.handleEvent(event);
                if (event.__immediate) break;
            }
            const handler = dispatchTarget['on' + event.type];
            if (!event.__immediate && typeof handler === 'function') handler.call(dispatchTarget, event);
            return !event.defaultPrevented;
        }
    }

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

