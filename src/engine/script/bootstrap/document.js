    const htmlNamespace = 'http://www.w3.org/1999/xhtml';
    const svgNamespace = 'http://www.w3.org/2000/svg';
    const knownHtmlElements = new Set((
        'html head title base link meta style body article section nav aside h1 h2 h3 h4 h5 h6 ' +
        'hgroup header footer address p hr pre blockquote ol ul menu li dl dt dd figure figcaption ' +
        'main search div a em strong small s cite q dfn abbr ruby rt rp data time code var samp kbd ' +
        'sub sup i b u mark bdi bdo span br wbr ins del picture source img iframe embed object video ' +
        'audio track map area table caption colgroup col tbody thead tfoot tr td th form label input ' +
        'button select datalist optgroup option textarea output progress meter fieldset legend details ' +
        'summary dialog script noscript template slot canvas acronym applet basefont bgsound big blink ' +
        'center content dir font frame frameset image keygen marquee menuitem nobr noembed noframes ' +
        'param plaintext rb rtc shadow spacer strike tt xmp'
    ).split(/\s+/));
    const htmlElementConstructor = localName => {
        if (localName === 'body') return HTMLBodyElement;
        if (localName === 'frameset') return HTMLFrameSetElement;
        if (localName === 'div') return HTMLDivElement;
        if (localName === 'html') return HTMLHtmlElement;
        if (localName === 'p') return HTMLParagraphElement;
        if (localName === 'title') return HTMLTitleElement;
        if (localName === 'style') return HTMLStyleElement;
        if (localName === 'link') return HTMLLinkElement;
        if (localName === 'time') return HTMLTimeElement;
        if (localName === 'data') return HTMLDataElement;
        if (localName === 'a') return HTMLAnchorElement;
        if (localName === 'details') return HTMLDetailsElement;
        if (localName === 'dialog') return HTMLDialogElement;
        if (localName === 'script') return HTMLScriptElement;
        if (localName === 'img') return HTMLImageElement;
        if (localName === 'picture') return HTMLPictureElement;
        if (localName === 'source') return HTMLSourceElement;
        if (localName === 'iframe') return HTMLIFrameElement;
        if (localName === 'video') return HTMLVideoElement;
        if (localName === 'audio') return HTMLAudioElement;
        if (localName === 'canvas') return HTMLCanvasElement;
        if (localName === 'input') return HTMLInputElement;
        if (localName === 'textarea') return HTMLTextAreaElement;
        if (localName === 'ol') return HTMLOrderedListElement;
        if (localName === 'select') return HTMLSelectElement;
        if (localName === 'option') return HTMLOptionElement;
        if (localName === 'button') return HTMLButtonElement;
        if (localName === 'label') return HTMLLabelElement;
        if (localName === 'fieldset') return HTMLFieldSetElement;
        if (localName === 'datalist') return HTMLDataListElement;
        if (localName === 'output') return HTMLOutputElement;
        if (localName === 'progress') return HTMLProgressElement;
        if (localName === 'meter') return HTMLMeterElement;
        if (localName === 'template') return HTMLTemplateElement;
        if (localName === 'slot') return HTMLSlotElement;
        if (localName === 'form') return HTMLFormElement;
        return knownHtmlElements.has(localName) || localName.includes('-')
            ? HTMLElement
            : HTMLUnknownElement;
    };
    class DOMImplementation {
        createDocument(namespace, qualifiedName, doctype = null) {
            if (doctype !== null) throw new DOMException('DocumentType insertion is not implemented', 'NotSupportedError');
            const result = wrap(host('createDocument', namespace == null ? '' : String(namespace), String(qualifiedName)));
            Object.setPrototypeOf(result, XMLDocument.prototype);
            return result;
        }
        createHTMLDocument(title = '') { return wrap(host('createHtmlDocument', String(title))); }
    }

    let throwOnDynamicMarkupInsertion = 0;
    const documentDefaultViews = new WeakMap();
    const documentReadiness = new WeakMap();
    const documentCollections = new WeakMap();
    const documentCollection = (document, name, selector) => {
        let collections = documentCollections.get(document);
        if (!collections) {
            collections = new Map();
            documentCollections.set(document, collections);
        }
        if (!collections.has(name)) collections.set(name, selectorCollection(document, selector));
        return collections.get(name);
    };
    class Document extends Node {
        constructor(id = 0, ...metadata) {
            super(Number(id) || host('createDocument', '', ''), ...metadata);
            cache.set(this.__id, this);
            documentReadiness.set(this, host('isPrimaryDocument', this.__id) ? 'loading' : 'complete');
            this.activeElement = null;
            this._currentScript = null;
        }
        createElement(name) {
            return maybeUpgradeCustomElement(wrap(host('createElement', this.__id, String(name))), true);
        }
        createElementNS(namespace, name) {
            const element = wrap(host('createElementNS', this.__id,
                namespace == null ? '' : String(namespace), String(name)));
            return maybeUpgradeCustomElement(element, true);
        }
        createTextNode(text) { return wrap(host('createText', this.__id, String(text))); }
        createComment(text) { return wrap(host('createComment', this.__id, String(text))); }
        createDocumentFragment() { return wrap(host('createDocumentFragment', this.__id)); }
        createTreeWalker(root, whatToShow = NodeFilter.SHOW_ALL, filter = null) {
            if (!(root instanceof Node)) throw new TypeError('createTreeWalker requires a Node root');
            return new TreeWalker(treeWalkerToken, root, whatToShow, filter);
        }
        createAttribute(localName) { return createAttributeFor(this, localName); }
        createAttributeNS(namespace, qualifiedName) { return createAttributeNsFor(this, namespace, qualifiedName); }
        importNode(node, deep = false) {
            if (!(node instanceof Node)) throw new TypeError('importNode requires a Node');
            const imported = wrap(host('importNode', this.__id, node.__id, !!deep));
            if (!imported) throw new DOMException('Documents cannot be imported', 'NotSupportedError');
            upgradeCustomElementTree(imported);
            return imported;
        }
        adoptNode(node) {
            if (!(node instanceof Node)) throw new TypeError('adoptNode requires a Node');
            if (node instanceof Document)
                throw new DOMException('Documents cannot be adopted', 'NotSupportedError');
            const oldDocument = node.ownerDocument;
            const oldParent = node.parentNode;
            const wasConnected = node.isConnected;
            if (wasConnected) disconnectElementTree(node);
            const adopted = wrap(host('adoptNode', this.__id, node.__id));
            if (!adopted) throw new DOMException('The node cannot be adopted', 'NotSupportedError');
            markChildCollectionsChanged(oldParent);
            if (oldDocument !== this) adoptCustomElementTree(adopted, oldDocument, this);
            return adopted;
        }
        createEvent(type) {
            const interfaceName = String(type).toLowerCase();
            const event = interfaceName === 'customevent' ? new CustomEvent('') :
                interfaceName === 'beforeunloadevent' ? new BeforeUnloadEvent(beforeUnloadConstructionToken) :
                interfaceName === 'storageevent' ? new StorageEvent('') :
                interfaceName === 'messageevent' ? new MessageEvent('') : new Event('');
            event.__initialized = false;
            return event;
        }
        getElementById(id) { return wrap(host('byId', this.__id, String(id))); }
        getElementsByTagName(name) { return selectorCollection(this, String(name)); }
        getElementsByClassName(name) {
            return selectorCollection(this, '.' + String(name).trim().replace(/\s+/g, '.'));
        }
        getElementsByName(name) { return this.querySelectorAll('[name="' + String(name).replace(/"/g, '\\"') + '"]'); }
        // These document collections are live and retain identity as required by HTML.
        // https://html.spec.whatwg.org/multipage/dom.html#dom-document-scripts
        get images() { return documentCollection(this, 'images', 'img'); }
        get embeds() { return documentCollection(this, 'embeds', 'embed'); }
        get plugins() { return this.embeds; }
        get links() { return documentCollection(this, 'links', 'a[href],area[href]'); }
        get forms() { return documentCollection(this, 'forms', 'form'); }
        get scripts() { return documentCollection(this, 'scripts', 'script'); }
        get documentElement() { return this.children[0] || null; }
        get doctype() { return wrap(host('doctype', this.__id)); }
        get head() { return this.querySelector('head'); }
        get body() { return this.querySelector('body'); }
        get title() { return documentTitleValue(this); }
        set title(value) { setDocumentTitleValue(this, value); }
        get URL() { return host('documentUrl', this.__id); }
        get documentURI() { return this.URL; }
        get baseURI() {
            if (host('isPrimaryDocument', this.__id)) return host('apiBaseUrl');
            const base = this.querySelector('base[href]');
            try { return base ? host('strictResolveUrl', base.getAttribute('href'), this.URL) : this.URL; }
            catch (_) { return this.URL; }
        }
        get location() { return this.defaultView?.location || null; }
        set location(value) { if (this.defaultView) this.defaultView.location.href = String(value); }
        get currentScript() { return this._currentScript; }
        get defaultView() {
            return documentDefaultViews.get(this) ||
                (host('isPrimaryDocument', this.__id) ? windowObject : null);
        }
        get implementation() { return this.__implementation ||= new DOMImplementation(); }
        __setCurrentScript(id) { this._currentScript = wrap(id); }
        __dispatchNodeEvent(id, type) {
            const target = wrap(id);
            if (target) target.dispatchEvent(markTrusted(new Event(String(type))));
        }
        write(...parts) {
            const text = parts.map(value => {
                if (typeof value === 'symbol') throw new TypeError('Cannot convert a Symbol to a string');
                return String(value);
            }).join('');
            checkDynamicMarkupTarget(this);
            if (writeIntoActiveParser(this, text)) return;
            if (host('ignoreDestructiveWrite')) return;
            openDocumentStream(this);
            writeIntoActiveParser(this, text);
        }
        writeln(...parts) { this.write(...parts, '\n'); }
        open(...args) {
            if (args.length >= 3) {
                if (!(this instanceof Document)) throw new TypeError('Illegal invocation');
                if (!this.defaultView) throw new DOMException('This document has no window', 'InvalidAccessError');
                throw new DOMException('Opening auxiliary browsing contexts is not supported', 'NotSupportedError');
            }
            checkDynamicMarkupTarget(this);
            return openDocumentStream(this);
        }
        close() {
            checkDynamicMarkupTarget(this);
            if (host('documentClose', this.__id)) pumpDocumentParser(this, true);
        }
        hasFocus() { return true; }
        get hidden() { return false; }
        get visibilityState() { return 'visible'; }
        get compatMode() { return host('documentCompatMode', this.__id); }
        get characterSet() { return host('documentCharacterSet', this.__id); }
        get charset() { return this.characterSet; }
        get inputEncoding() { return this.characterSet; }
        get contentType() { return host('documentContentType', this.__id); }
        // Documents without a browsing context are cookie-averse (HTML resource metadata).
        get cookie() { return this.defaultView ? host('cookieGet') : ''; }
        set cookie(value) { if (this.defaultView) host('cookieSet', String(value)); }
    }
    class XMLDocument extends Document {}
    installParentNodeMembers(Document.prototype);
    const parserDomChanged = (ids, mutations = []) => {
        parserCollectionEpoch++;
        for (const record of mutations) {
            for (const id of [...record.added, ...record.removed]) invalidateMutationAncestors(wrap(id));
            queueMutationRecord(wrap(record.target), record.type, {
                addedNodes: record.added.map(wrap), removedNodes: record.removed.map(wrap),
                previousSibling: wrap(record.previous), nextSibling: wrap(record.next),
                attributeName: record.name, attributeNamespace: record.namespace,
                oldValue: record.oldValue
            }, record.ancestors.map(wrap));
        }
        for (const node of list(ids)) {
            if (node.localName === 'iframe') host('frameWindow', node.__id, node);
            maybeUpgradeCustomElement(node, false, true);
        }
        refreshParserEventHandlerAttributes();
        refreshWindowNamedProperties();
    };
    globalThis.__parserDomChanged = parserDomChanged;
    installEventHandlerAttributes(Document.prototype);

    function wrap(id) {
        id = Number(id) || 0;
        if (!id) return null;
        if (cache.has(id)) return cache.get(id);
        const metadata = host('nodeMetadata', id).split('\u001f');
        const type = Number(metadata[0]);
        let node;
        if (type === 9) node = new Document(id, type, metadata[1], null, null);
        else if (type === 1) {
            const namespace = metadata[3] || null;
            const Constructor = namespace === htmlNamespace
                ? htmlElementConstructor(metadata[2])
                : namespace === svgNamespace
                    ? metadata[2] === 'svg' ? SVGSVGElement : SVGElement
                    : Element;
            node = Constructor === HTMLTitleElement
                ? new HTMLTitleElement(id, type, metadata[1], metadata[2], namespace, htmlTitleConstructionToken)
                : new Constructor(id, type, metadata[1], metadata[2] || null, namespace);
        }
        else if (type === 10) node = new DocumentType(id, type, metadata[1], null, null);
        else if (type === 11) node = metadata[4] === 'shadow'
            ? new ShadowRoot(id, type, metadata[1], null, null, shadowRootConstructionToken)
            : new DocumentFragment(id, type, metadata[1], null, null);
        else if (type === 4) node = new CDATASection(id, type, metadata[1], null, null);
        else if (type === 7) node = new ProcessingInstruction(id, type, metadata[1], null, null);
        else if (type === 8) node = new Comment(id, type, metadata[1], null, null);
        else node = new Text(id, type, metadata[1], null, null);
        cache.set(id, node);
        if (metadata[5] === 'handlers') initializeEventHandlerAttributes(node);
        return node;
    }

    const document = wrap(host('document'));
