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
        constructor(id, ...metadata) {
            if (id === undefined) return constructCustomElement(new.target);
            super(id, ...metadata);
        }
        get dataset() { return this.__dataset ||= datasetFor(this); }
        // HTML defines innerText on HTMLElement, not Node. The complete getter is
        // layout-aware; until whitespace and generated-line handling cross the host
        // boundary, preserve the required DOMString contract with the subtree text.
        // This is also the specified fallback when an element is not being rendered.
        get innerText() { return this.textContent || ''; }
        set innerText(value) { this.textContent = value == null ? '' : String(value); }
    }
    installEventHandlerAttributes(HTMLElement.prototype);
    class HTMLBodyElement extends HTMLElement {}
    class HTMLFrameSetElement extends HTMLElement {}
    for (const prototype of [HTMLBodyElement.prototype, HTMLFrameSetElement.prototype])
        for (const type of windowHandlerTypes) defineEventHandler(prototype, null, type);
    class HTMLDivElement extends HTMLElement {}
    class HTMLStyleElement extends HTMLElement {
        get media() { return this.getAttribute('media') || ''; }
        set media(value) { this.setAttribute('media', value); }
        get type() { return this.getAttribute('type') || ''; }
        set type(value) { this.setAttribute('type', value); }
        get disabled() { return this.hasAttribute('disabled'); }
        set disabled(value) { this.toggleAttribute('disabled', !!value); }
    }
    class HTMLLinkElement extends HTMLElement {
        get rel() { return this.getAttribute('rel') || ''; }
        set rel(value) { this.setAttribute('rel', value); }
        get relList() { return this.__relList ||= new DOMTokenList(this, 'rel'); }
        get media() { return this.getAttribute('media') || ''; }
        set media(value) { this.setAttribute('media', value); }
        get type() { return this.getAttribute('type') || ''; }
        set type(value) { this.setAttribute('type', value); }
        get disabled() { return this.hasAttribute('disabled'); }
        set disabled(value) { this.toggleAttribute('disabled', !!value); }
    }
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
    class HTMLImageElement extends HTMLElement {
        get complete() { return imageElementState(this).complete; }
        get currentSrc() { return imageElementState(this).source; }
        get naturalWidth() { return imageElementState(this).naturalWidth; }
        get naturalHeight() { return imageElementState(this).naturalHeight; }
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
