    class Element extends Node {
        get tagName() { return this.__nodeName; }
        get localName() { return this.__localName; }
        get namespaceURI() { return this.__namespaceURI; }
        get prefix() { return host('prefix', this.__id); }
        get id() { return this.getAttribute('id') || ''; }
        set id(value) { this.setAttribute('id', value); }
        get slot() { return this.getAttribute('slot') || ''; }
        set slot(value) { this.setAttribute('slot', value); }
        get className() { return this.getAttribute('class') || ''; }
        set className(value) { this.setAttribute('class', value); }
        get classList() { return this.__classList ||= new DOMTokenList(this, 'class'); }
        get style() { return this.__style ||= styleProxy(this); }
        attachShadow(init) {
            if (init == null) throw new TypeError('attachShadow requires an options dictionary');
            init = Object(init);
            const mode = String(init.mode);
            if (mode !== 'open' && mode !== 'closed') throw new TypeError('mode must be open or closed');
            const slotAssignment = init.slotAssignment === undefined ? 'named' : String(init.slotAssignment);
            if (slotAssignment !== 'named')
                throw new DOMException('Manual slot assignment is not implemented', 'NotSupportedError');
            const validBuiltIn = new Set([
                'article', 'aside', 'blockquote', 'body', 'div', 'footer', 'h1', 'h2', 'h3',
                'h4', 'h5', 'h6', 'header', 'main', 'nav', 'p', 'section', 'span'
            ]).has(this.localName);
            const validCustomName = /^[a-z][.0-9_a-z-]*-[.0-9_a-z-]*$/.test(this.localName) &&
                !new Set(['annotation-xml', 'color-profile', 'font-face', 'font-face-src',
                    'font-face-uri', 'font-face-format', 'font-face-name', 'missing-glyph']).has(this.localName);
            if (this.namespaceURI !== htmlNamespace || (!validBuiltIn && !validCustomName))
                throw new DOMException('This element cannot host a shadow tree', 'NotSupportedError');
            const root = wrap(host('attachShadow', this.__id, mode, !!init.delegatesFocus,
                !!init.serializable, !!init.clonable));
            if (!root) throw new DOMException('This element already hosts a shadow tree', 'NotSupportedError');
            scheduleSlotChangeCheck();
            return root;
        }
        get shadowRoot() { return wrap(host('shadowRoot', this.__id)); }
        get innerHTML() { return host('innerHtmlGet', this.__id); }
        set innerHTML(value) {
            const wasConnected = this.isConnected;
            const removedChildren = wasConnected ? [...this.childNodes] : [];
            host('innerHtmlSet', this.__id, value == null ? '' : String(value));
            markChildCollectionsChanged(this);
            for (const child of removedChildren) disconnectCustomElementTree(child);
            for (const child of this.childNodes) {
                if (wasConnected) connectCustomElementTree(child);
                else upgradeCustomElementTree(child);
            }
            if (wasConnected) refreshWindowNamedProperties(removedChildren.concat(this.childNodes));
            scheduleSlotChangeCheck();
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
        getAttribute(name) {
            name = normalizedQualifiedName(this, name);
            return host('attrGet', this.__id, name);
        }
        getAttributeNS(namespace, localName) {
            namespace = normalizedNamespace(namespace);
            return host('attrGetNs', this.__id, namespace || '', String(localName));
        }
        setAttribute(name, value) {
            name = normalizedQualifiedName(this, validateAttributeLocalName(String(name)));
            const record = recordByQualifiedName(this, name);
            const oldValue = record?.value ?? null;
            host('attrSet', this.__id, name, String(value));
            const current = recordByQualifiedName(this, name);
            queueAttributeMutation(this, current, oldValue);
            maybeRefreshNamedProperties(this, current.namespace, current.localName, oldValue, current.value);
            scheduleSlotChangeCheck();
        }
        setAttributeNS(namespace, qualifiedName, value) {
            const extracted = validateAndExtractAttributeName(namespace, qualifiedName);
            const record = recordByNamespace(this, extracted.namespace, extracted.localName);
            const oldValue = record?.value ?? null;
            host('attrSetNs', this.__id, extracted.namespace || '', extracted.prefix || '',
                extracted.localName, String(value));
            const current = recordByNamespace(this, extracted.namespace, extracted.localName);
            queueAttributeMutation(this, current, oldValue);
            maybeRefreshNamedProperties(this, current.namespace, current.localName, oldValue, current.value);
            scheduleSlotChangeCheck();
        }
        removeAttribute(name) {
            name = normalizedQualifiedName(this, name);
            const record = recordByQualifiedName(this, name);
            if (!record) return;
            const attribute = attrForRecord(this, record);
            host('attrRemove', this.__id, name);
            detachAttribute(this, record, attribute);
            queueAttributeMutation(this, record, record.value);
            maybeRefreshNamedProperties(this, record.namespace, record.localName, record.value, null);
            scheduleSlotChangeCheck();
        }
        removeAttributeNS(namespace, localName) {
            const record = recordByNamespace(this, namespace, localName);
            if (!record) return;
            const attribute = attrForRecord(this, record);
            host('attrRemoveNs', this.__id, record.namespace || '', record.localName);
            detachAttribute(this, record, attribute);
            queueAttributeMutation(this, record, record.value);
            maybeRefreshNamedProperties(this, record.namespace, record.localName, record.value, null);
            scheduleSlotChangeCheck();
        }
        hasAttribute(name) { return host('attrHas', this.__id, normalizedQualifiedName(this, name)); }
        hasAttributeNS(namespace, localName) {
            namespace = normalizedNamespace(namespace);
            return host('attrHasNs', this.__id, namespace || '', String(localName));
        }
        hasAttributes() { return this.attributes.length !== 0; }
        toggleAttribute(name, force) {
            name = normalizedQualifiedName(this, validateAttributeLocalName(String(name)));
            const present = this.hasAttribute(name);
            if (force === true || (!present && force !== false)) { this.setAttribute(name, ''); return true; }
            if (present) this.removeAttribute(name);
            return false;
        }
        getAttributeNames() {
            return attributeRecords(this).map(record => record.qualifiedName);
        }
        get attributes() { return attributeMapFor(this); }
        getAttributeNode(qualifiedName) { return getAttributeNodeFor(this, qualifiedName); }
        getAttributeNodeNS(namespace, localName) { return getAttributeNodeNsFor(this, namespace, localName); }
        setAttributeNode(attribute) { return setAttributeNodeFor(this, attribute); }
        setAttributeNodeNS(attribute) { return setAttributeNodeFor(this, attribute); }
        removeAttributeNode(attribute) { return removeAttributeNodeFor(this, attribute); }
        matches(selector) { return host('matches', this.__id, String(selector)); }
        closest(selector) { return wrap(host('closest', this.__id, String(selector))); }
        getElementsByTagName(name) { return selectorCollection(this, String(name)); }
        getElementsByClassName(name) {
            return selectorCollection(this, '.' + String(name).trim().replace(/\s+/g, '.'));
        }
        insertAdjacentHTML(position, html) {
            position = String(position).toLowerCase();
            if (position === 'beforeend') {
                const previousChildren = new Set(this.childNodes.map(child => child.__id));
                host('innerHtmlAppend', this.__id, String(html));
                markChildCollectionsChanged(this);
                for (const child of this.childNodes) if (!previousChildren.has(child.__id)) {
                    if (this.isConnected) connectCustomElementTree(child);
                    else upgradeCustomElementTree(child);
                }
                if (this.isConnected) refreshWindowNamedProperties(
                    this.childNodes.filter(child => !previousChildren.has(child.__id)));
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

    class SVGAnimatedString {
        constructor(element, attribute) {
            this.__element = element;
            this.__attribute = attribute;
        }
        get baseVal() { return this.__element.getAttribute(this.__attribute) || ''; }
        set baseVal(value) { this.__element.setAttribute(this.__attribute, String(value)); }
        get animVal() { return this.baseVal; }
    }
    class SVGElement extends Element {
        get className() {
            return this.__className ||= new SVGAnimatedString(this, 'class');
        }
        set className(value) { this.__classNameValue.baseVal = value; }
        get __classNameValue() {
            return this.__className ||= new SVGAnimatedString(this, 'class');
        }
        get ownerSVGElement() {
            let ancestor = this.parentElement;
            while (ancestor && !(ancestor instanceof SVGSVGElement)) ancestor = ancestor.parentElement;
            return ancestor;
        }
        get viewportElement() { return this.ownerSVGElement; }
    }
    class SVGSVGElement extends SVGElement {}

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
    }
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
        constructor(id, ...metadata) {
            super(id, ...metadata);
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
