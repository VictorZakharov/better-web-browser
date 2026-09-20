    const imageElementStates = new WeakMap();
    const resetImageElementState = element => imageElementStates.delete(element);
    const updateImageElementState = (element, complete, naturalWidth, naturalHeight) => {
        imageElementStates.set(element, {
            source: element.src,
            complete: !!complete,
            naturalWidth: Math.max(0, Number(naturalWidth) || 0),
            naturalHeight: Math.max(0, Number(naturalHeight) || 0),
        });
    };
    const imageElementState = element => {
        const source = element.src;
        const state = imageElementStates.get(element);
        if (state?.source === source) return state;
        return {
            source,
            complete: !element.hasAttribute('src') && !element.srcset,
            naturalWidth: 0,
            naturalHeight: 0,
        };
    };

    class Element extends Node {
        get tagName() { return this.__nodeName; }
        get localName() { return this.__localName; }
        get namespaceURI() { return this.__namespaceURI; }
        get prefix() { return host('prefix', nodeId(this)); }
        get previousElementSibling() { return elementSibling(this, false); }
        get nextElementSibling() { return elementSibling(this, true); }
        get id() { return this.getAttribute('id') || ''; }
        set id(value) { this.setAttribute('id', value); }
        get slot() { return this.getAttribute('slot') || ''; }
        set slot(value) { this.setAttribute('slot', value); }
        get className() { return this.getAttribute('class') || ''; }
        set className(value) { this.setAttribute('class', value); }
        get classList() { return this.__classList ||= new DOMTokenList(this, 'class'); }
        get style() { return this.__style ||= styleProxy(this); }
        // ElementCSSInlineStyle declares [PutForwards=cssText]. Assigning `element.style`
        // therefore updates the existing declaration instead of replacing the same-object value.
        set style(value) { this.style.cssText = value; }
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
            const root = wrap(host('attachShadow', nodeId(this), mode, !!init.delegatesFocus,
                !!init.serializable, !!init.clonable));
            if (!root) throw new DOMException('This element already hosts a shadow tree', 'NotSupportedError');
            scheduleSlotChangeCheck();
            return root;
        }
        get shadowRoot() { return wrap(host('shadowRoot', nodeId(this))); }
        get innerHTML() { return host('innerHtmlGet', nodeId(this)); }
        set innerHTML(value) { replaceElementInnerHtml(this, value); }
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
            return host('attrGet', nodeId(this), name);
        }
        getAttributeNS(namespace, localName) {
            namespace = normalizedNamespace(namespace);
            return host('attrGetNs', nodeId(this), namespace || '', String(localName));
        }
        setAttribute(name, value) {
            name = normalizedQualifiedName(this, validateAttributeLocalName(String(name)));
            value = String(value);
            const record = host('attrSet', nodeId(this), name, value);
            const oldValue = record?.value ?? null;
            const current = record ? { ...record, value } : {
                namespace: null, prefix: null, localName: name, qualifiedName: name, value
            };
            if (this.localName === 'img' &&
                (current.localName === 'src' || current.localName === 'srcset')) {
                resetImageElementState(this);
            }
            queueAttributeMutation(this, current, oldValue, value);
            maybeRefreshNamedProperties(this, current.namespace, current.localName, oldValue, value);
            maybeRefreshPatternVerdict(this, current.localName);
            scheduleSlotChangeCheck();
        }
        setAttributeNS(namespace, qualifiedName, value) {
            const extracted = validateAndExtractAttributeName(namespace, qualifiedName);
            value = String(value);
            const record = host('attrSetNs', nodeId(this), extracted.namespace || '',
                extracted.prefix || '', extracted.localName, value);
            const oldValue = record?.value ?? null;
            const current = record ? { ...record, value } : { ...extracted, value };
            if (this.localName === 'img' && current.namespace === null &&
                (current.localName === 'src' || current.localName === 'srcset'))
                resetImageElementState(this);
            queueAttributeMutation(this, current, oldValue, value);
            maybeRefreshNamedProperties(this, current.namespace, current.localName, oldValue, value);
            if (current.namespace === null) maybeRefreshPatternVerdict(this, current.localName);
            scheduleSlotChangeCheck();
        }
        removeAttribute(name) {
            name = normalizedQualifiedName(this, name);
            const record = host('attrRemove', nodeId(this), name);
            if (!record) return;
            if (this.localName === 'img' &&
                (record.localName === 'src' || record.localName === 'srcset'))
                resetImageElementState(this);
            detachCachedAttribute(this, record);
            queueAttributeMutation(this, record, record.value, null);
            maybeRefreshNamedProperties(this, record.namespace, record.localName, record.value, null);
            maybeRefreshPatternVerdict(this, record.localName);
            scheduleSlotChangeCheck();
        }
        removeAttributeNS(namespace, localName) {
            namespace = normalizedNamespace(namespace);
            localName = String(localName);
            const record = host('attrRemoveNs', nodeId(this), namespace || '', localName);
            if (!record) return;
            if (this.localName === 'img' && record.namespace === null &&
                (record.localName === 'src' || record.localName === 'srcset'))
                resetImageElementState(this);
            detachCachedAttribute(this, record);
            queueAttributeMutation(this, record, record.value, null);
            maybeRefreshNamedProperties(this, record.namespace, record.localName, record.value, null);
            if (record.namespace === null) maybeRefreshPatternVerdict(this, record.localName);
            scheduleSlotChangeCheck();
        }
        hasAttribute(name) { return host('attrHas', nodeId(this), normalizedQualifiedName(this, name)); }
        hasAttributeNS(namespace, localName) {
            namespace = normalizedNamespace(namespace);
            return host('attrHasNs', nodeId(this), namespace || '', String(localName));
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
        matches(selector) { return host('matches', nodeId(this), String(selector)); }
        closest(selector) { return wrap(host('closest', nodeId(this), String(selector))); }
        getElementsByTagName(name) { return selectorCollection(this, String(name)); }
        getElementsByClassName(name) {
            return selectorCollection(this, '.' + String(name).trim().replace(/\s+/g, '.'));
        }
        insertAdjacentHTML(position, html) {
            position = String(position).toLowerCase();
            if (position === 'beforeend') {
                const previousChildren = new Set(this.childNodes.map(child => nodeId(child)));
                host('innerHtmlAppend', nodeId(this), String(html));
                markChildCollectionsChanged(this);
                for (const child of this.childNodes) if (!previousChildren.has(nodeId(child))) {
                    if (this.isConnected) connectElementTree(child);
                    else upgradeCustomElementTree(child);
                }
                if (this.isConnected) refreshWindowNamedProperties(
                    this.childNodes.filter(child => !previousChildren.has(nodeId(child))));
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
        get contentWindow() {
            return this.localName === 'iframe' ? host('frameWindow', nodeId(this), this) : null;
        }
        get contentDocument() {
            return this.localName === 'iframe' ? host('frameDocument', nodeId(this), this) : null;
        }
        click() {
            if (this.__clicking || this.matches(':disabled')) return;
            this.__clicking = true;
            let allowed;
            try { allowed = this.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, composed: true })); }
            finally { this.__clicking = false; }
            if (allowed && this.localName === 'summary') {
                const details = this.parentElement;
                const firstSummary = Array.from(details?.children || [])
                    .find(child => child.localName === 'summary');
                if (details instanceof HTMLDetailsElement && firstSummary === this) details.open = !details.open;
            }
        }
        focus() { document.activeElement = this; this.dispatchEvent(new Event('focus')); }
        blur() { if (document.activeElement === this) document.activeElement = document.body; this.dispatchEvent(new Event('blur')); }
    }
    installParentNodeMembers(Element.prototype);
    installChildNodeMembers(Element.prototype);

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
    installEventHandlerAttributes(SVGElement.prototype);
