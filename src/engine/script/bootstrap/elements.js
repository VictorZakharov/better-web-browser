    class Element extends Node {
        get tagName() { return host('tagName', this.__id); }
        get localName() { return host('localName', this.__id); }
        get namespaceURI() { return host('namespaceUri', this.__id); }
        get prefix() { return host('prefix', this.__id); }
        get id() { return this.getAttribute('id') || ''; }
        set id(value) { this.setAttribute('id', value); }
        get className() { return this.getAttribute('class') || ''; }
        set className(value) { this.setAttribute('class', value); }
        get classList() { return this.__classList ||= new DOMTokenList(this, 'class'); }
        get style() { return this.__style ||= styleProxy(this); }
        get innerHTML() { return host('innerHtmlGet', this.__id); }
        set innerHTML(value) {
            host('innerHtmlSet', this.__id, value == null ? '' : String(value));
            if (this.isConnected) refreshWindowNamedProperties();
        }
        get outerHTML() { return '<' + this.localName + '>' + this.innerHTML + '</' + this.localName + '>'; }
        set outerHTML(value) {
            const parent = this.parentNode;
            if (!parent) return;
            const holder = document.createElement('div');
            holder.innerHTML = value == null ? '' : String(value);
            for (const child of [...holder.childNodes]) parent.insertBefore(child, this);
            parent.removeChild(this);
        }
        getAttribute(name) { return host('attrGet', this.__id, String(name)); }
        setAttribute(name, value) {
            name = String(name);
            host('attrSet', this.__id, name, String(value));
            if (this.isConnected && (name.toLowerCase() === 'id' || name.toLowerCase() === 'name'))
                refreshWindowNamedProperties();
        }
        removeAttribute(name) {
            name = String(name);
            host('attrRemove', this.__id, name);
            if (this.isConnected && (name.toLowerCase() === 'id' || name.toLowerCase() === 'name'))
                refreshWindowNamedProperties();
        }
        hasAttribute(name) { return host('attrHas', this.__id, String(name)); }
        toggleAttribute(name, force) {
            const present = this.hasAttribute(name);
            if (force === true || (!present && force !== false)) { this.setAttribute(name, ''); return true; }
            if (present) this.removeAttribute(name);
            return false;
        }
        getAttributeNames() {
            const names = host('attrNames', this.__id);
            return names ? names.split('\u001f') : [];
        }
        get attributes() {
            const element = this;
            const attributes = this.getAttributeNames().map(name => ({
                name,
                nodeName: name,
                get value() { return element.getAttribute(name) || ''; },
                set value(value) { element.setAttribute(name, value); },
                get nodeValue() { return this.value; },
                set nodeValue(value) { this.value = value; },
                ownerElement: element,
                specified: true
            }));
            attributes.item = index => attributes[index] || null;
            attributes.getNamedItem = name => attributes.find(attribute => attribute.name === String(name)) || null;
            return attributes;
        }
        matches(selector) { return this.parentNode?.querySelectorAll(selector).includes(this) || false; }
        closest(selector) {
            for (let node = this; node?.nodeType === 1; node = node.parentElement) if (node.matches(selector)) return node;
            return null;
        }
        getElementsByTagName(name) { return this.querySelectorAll(String(name)); }
        getElementsByClassName(name) { return this.querySelectorAll('.' + String(name).trim().replace(/\s+/g, '.')); }
        insertAdjacentHTML(position, html) {
            position = String(position).toLowerCase();
            if (position === 'beforeend') {
                host('innerHtmlAppend', this.__id, String(html));
                if (this.isConnected) refreshWindowNamedProperties();
            }
            else if (position === 'afterbegin') this.innerHTML = String(html) + this.innerHTML;
            else if (position === 'beforebegin' && this.parentNode) {
                const holder = document.createElement('div'); holder.innerHTML = String(html);
                for (const child of [...holder.childNodes]) this.parentNode.insertBefore(child, this);
            } else if (position === 'afterend' && this.parentNode) {
                const holder = document.createElement('div'); holder.innerHTML = String(html);
                let reference = this.nextSibling;
                for (const child of [...holder.childNodes]) this.parentNode.insertBefore(child, reference);
            }
        }
        insertAdjacentText(position, text) { this.insertAdjacentHTML(position, String(text).replace(/&/g, '&amp;').replace(/</g, '&lt;')); }
        get href() { const value = this.getAttribute('href'); return value == null ? '' : host('resolveUrl', value); }
        set href(value) { this.setAttribute('href', value); }
        get src() { const value = this.getAttribute('src'); return value == null ? '' : host('resolveUrl', value); }
        set src(value) { this.setAttribute('src', value); }
        get value() { return this.getAttribute('value') || ''; }
        set value(value) { this.setAttribute('value', value); }
        get name() { return this.getAttribute('name') || ''; }
        set name(value) { this.setAttribute('name', value); }
        get type() { return this.getAttribute('type') || ''; }
        set type(value) { this.setAttribute('type', value); }
        get checked() { return this.hasAttribute('checked'); }
        set checked(value) { this.toggleAttribute('checked', !!value); }
        get disabled() { return this.hasAttribute('disabled'); }
        set disabled(value) { this.toggleAttribute('disabled', !!value); }
        get hidden() { return this.hasAttribute('hidden'); }
        set hidden(value) { this.toggleAttribute('hidden', !!value); }
        get dataset() {
            const element = this;
            return new Proxy({}, {
                get(_, property) { return element.getAttribute('data-' + String(property).replace(/[A-Z]/g, match => '-' + match.toLowerCase())); },
                set(_, property, value) { element.setAttribute('data-' + String(property).replace(/[A-Z]/g, match => '-' + match.toLowerCase()), value); return true; }
            });
        }
        get contentWindow() { return this.localName === 'iframe' ? iframeWindow : null; }
        get contentDocument() { return this.localName === 'iframe' ? iframeDocument : null; }
        click() {
            const allowed = this.dispatchEvent(new Event('click', { bubbles: true, cancelable: true }));
            if (allowed && this.localName === 'summary') {
                const details = this.parentElement;
                const firstSummary = details?.children.find(child => child.localName === 'summary');
                if (details instanceof HTMLDetailsElement && firstSummary === this) details.open = !details.open;
            }
        }
        focus() { document.activeElement = this; this.dispatchEvent(new Event('focus')); }
        blur() { if (document.activeElement === this) document.activeElement = document.body; this.dispatchEvent(new Event('blur')); }
        get clientWidth() { return 0; }
        get clientHeight() { return 0; }
        get offsetWidth() { return 0; }
        get offsetHeight() { return 0; }
        getBoundingClientRect() { return { x: 0, y: 0, top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0, toJSON() { return this; } }; }
    }
    installEventHandlerAttributes(Element.prototype);

    const dataPropertyName = attribute => attribute.slice(5).replace(/-([a-z])/g, (_, letter) => letter.toUpperCase());
    const dataAttributeName = property => {
        property = String(property);
        if (/-[a-z]/.test(property)) throw new SyntaxError('dataset property names cannot contain a dash followed by a lowercase letter');
        return 'data-' + property.replace(/[A-Z]/g, letter => '-' + letter.toLowerCase());
    };
    class DOMStringMap {}
    const datasetFor = element => new Proxy(new DOMStringMap(), {
        get(target, property, receiver) {
            if (typeof property !== 'string' || property in target) return Reflect.get(target, property, receiver);
            const value = element.getAttribute(dataAttributeName(property));
            return value == null ? undefined : value;
        },
        set(_target, property, value) {
            if (typeof property !== 'string') return false;
            element.setAttribute(dataAttributeName(property), String(value));
            return true;
        },
        deleteProperty(_target, property) {
            if (typeof property === 'string') element.removeAttribute(dataAttributeName(property));
            return true;
        },
        has(target, property) {
            return property in target || (typeof property === 'string' && element.hasAttribute(dataAttributeName(property)));
        },
        ownKeys() {
            return element.getAttributeNames()
                .filter(name => name.startsWith('data-') && !/[A-Z]/.test(name.slice(5)))
                .map(dataPropertyName);
        },
        getOwnPropertyDescriptor(_target, property) {
            if (typeof property !== 'string' || !element.hasAttribute(dataAttributeName(property))) return undefined;
            return { configurable: true, enumerable: true, writable: true, value: element.getAttribute(dataAttributeName(property)) };
        }
    });
    class HTMLElement extends Element {
        get dataset() { return this.__dataset ||= datasetFor(this); }
    }
    class HTMLDivElement extends HTMLElement {}
    Object.defineProperties(HTMLElement.prototype, {
        translate: {
            configurable: true,
            get() {
                const value = this.getAttribute('translate');
                if (value == null || value === '') return this.parentElement?.translate ?? true;
                return value.toLowerCase() !== 'no';
            },
            set(value) { this.setAttribute('translate', value ? 'yes' : 'no'); }
        },
        accessKey: {
            configurable: true,
            get() { return this.getAttribute('accesskey') || ''; },
            set(value) { this.setAttribute('accesskey', value); }
        },
        accessKeyLabel: {
            configurable: true,
            get() { return ''; }
        }
    });
    class HTMLUnknownElement extends HTMLElement {}
    class HTMLTimeElement extends HTMLElement {
        get dateTime() { return this.getAttribute('datetime') || ''; }
        set dateTime(value) { this.setAttribute('datetime', value); }
    }
    class HTMLDataElement extends HTMLElement {
        get value() { return this.getAttribute('value') || ''; }
        set value(value) { this.setAttribute('value', value); }
    }
    class HTMLAnchorElement extends HTMLElement {
        get target() { return this.getAttribute('target') || ''; }
        set target(value) { this.setAttribute('target', value); }
        get download() { return this.getAttribute('download') || ''; }
        set download(value) { this.setAttribute('download', value); }
        get ping() { return this.getAttribute('ping') || ''; }
        set ping(value) { this.setAttribute('ping', value); }
        get rel() { return this.getAttribute('rel') || ''; }
        set rel(value) { this.setAttribute('rel', value); }
        get relList() { return this.__relList ||= new DOMTokenList(this, 'rel'); }
        get hreflang() { return this.getAttribute('hreflang') || ''; }
        set hreflang(value) { this.setAttribute('hreflang', value); }
        get type() { return this.getAttribute('type') || ''; }
        set type(value) { this.setAttribute('type', value); }
        get referrerPolicy() { return this.getAttribute('referrerpolicy') || ''; }
        set referrerPolicy(value) { this.setAttribute('referrerpolicy', value); }
        get text() { return this.textContent; }
        set text(value) { this.textContent = String(value); }
    }
    class HTMLDetailsElement extends HTMLElement {
        get open() { return this.hasAttribute('open'); }
        set open(value) {
            const wasOpen = this.open;
            const isOpen = !!value;
            if (wasOpen === isOpen) return;
            this.toggleAttribute('open', isOpen);
            setTimeout(() => this.dispatchEvent(new ToggleEvent('toggle', {
                oldState: wasOpen ? 'open' : 'closed',
                newState: isOpen ? 'open' : 'closed'
            })), 0);
        }
    }
    class HTMLDialogElement extends HTMLElement {
        constructor(id) {
            super(id);
            this.returnValue = '';
            this.__isModal = false;
        }
        get open() { return this.hasAttribute('open'); }
        set open(value) { this.toggleAttribute('open', !!value); }
        get closedBy() { return this.getAttribute('closedby') || 'none'; }
        set closedBy(value) { this.setAttribute('closedby', value); }
        show() {
            if (this.open) return;
            const event = new ToggleEvent('beforetoggle', {
                cancelable: true, oldState: 'closed', newState: 'open', source: this
            });
            if (!this.dispatchEvent(event)) return;
            this.open = true;
            this.focus();
            setTimeout(() => this.dispatchEvent(new ToggleEvent('toggle', {
                oldState: 'closed', newState: 'open', source: this
            })), 0);
        }
        showModal() {
            if (!this.isConnected) throw new DOMException('Dialog is not connected to a document', 'InvalidStateError');
            if (this.open) {
                if (!this.__isModal) throw new DOMException('Dialog is already open non-modally', 'InvalidStateError');
                return;
            }
            this.__isModal = true;
            this.show();
        }
        close(returnValue) {
            if (!this.open) return;
            if (returnValue !== undefined) this.returnValue = String(returnValue);
            this.__isModal = false;
            this.open = false;
            setTimeout(() => this.dispatchEvent(new Event('close')), 0);
        }
        requestClose(returnValue) {
            if (!this.open) return;
            if (this.dispatchEvent(new Event('cancel', { cancelable: true }))) this.close(returnValue);
        }
    }
    class HTMLScriptElement extends HTMLElement {
        get async() { return this.hasAttribute('async'); }
        set async(value) { this.toggleAttribute('async', !!value); }
        get defer() { return this.hasAttribute('defer'); }
        set defer(value) { this.toggleAttribute('defer', !!value); }
        get text() { return this.textContent; }
        set text(value) { this.textContent = String(value); }
    }
    class HTMLImageElement extends HTMLElement {
        get srcset() { return this.getAttribute('srcset') || ''; }
        set srcset(value) { this.setAttribute('srcset', value); }
        get sizes() { return this.getAttribute('sizes') || ''; }
        set sizes(value) { this.setAttribute('sizes', value); }
    }
    class HTMLPictureElement extends HTMLElement {}
    class HTMLSourceElement extends HTMLElement {
        get srcset() { return this.getAttribute('srcset') || ''; }
        set srcset(value) { this.setAttribute('srcset', value); }
        get sizes() { return this.getAttribute('sizes') || ''; }
        set sizes(value) { this.setAttribute('sizes', value); }
        get media() { return this.getAttribute('media') || ''; }
        set media(value) { this.setAttribute('media', value); }
    }
