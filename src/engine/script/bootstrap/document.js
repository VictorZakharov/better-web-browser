    const htmlNamespace = 'http://www.w3.org/1999/xhtml';
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
        if (localName === 'div') return HTMLDivElement;
        if (localName === 'time') return HTMLTimeElement;
        if (localName === 'data') return HTMLDataElement;
        if (localName === 'a') return HTMLAnchorElement;
        if (localName === 'details') return HTMLDetailsElement;
        if (localName === 'dialog') return HTMLDialogElement;
        if (localName === 'script') return HTMLScriptElement;
        if (localName === 'img') return HTMLImageElement;
        if (localName === 'picture') return HTMLPictureElement;
        if (localName === 'source') return HTMLSourceElement;
        if (localName === 'input') return HTMLInputElement;
        if (localName === 'textarea') return HTMLTextAreaElement;
        if (localName === 'ol') return HTMLOrderedListElement;
        if (localName === 'select') return HTMLSelectElement;
        if (localName === 'button') return HTMLButtonElement;
        if (localName === 'label') return HTMLLabelElement;
        if (localName === 'fieldset') return HTMLFieldSetElement;
        if (localName === 'datalist') return HTMLDataListElement;
        if (localName === 'output') return HTMLOutputElement;
        if (localName === 'progress') return HTMLProgressElement;
        if (localName === 'meter') return HTMLMeterElement;
        if (localName === 'template') return HTMLTemplateElement;
        if (localName === 'form') return HTMLFormElement;
        return knownHtmlElements.has(localName) || localName.includes('-')
            ? HTMLElement
            : HTMLUnknownElement;
    };

    class DOMImplementation {
        createDocument(namespace, qualifiedName, doctype = null) {
            if (doctype !== null) throw new DOMException('DocumentType insertion is not implemented', 'NotSupportedError');
            return wrap(host('createDocument', namespace == null ? '' : String(namespace), String(qualifiedName)));
        }
        createHTMLDocument(title = '') { return wrap(host('createHtmlDocument', String(title))); }
    }

    let documentWriteRefreshQueued = false;

    class Document extends Node {
        constructor(id = 0, ...metadata) {
            super(Number(id) || host('createDocument', '', ''), ...metadata);
            cache.set(this.__id, this);
            this.readyState = 'loading';
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
        createAttribute(localName) { return createAttributeFor(this, localName); }
        createAttributeNS(namespace, qualifiedName) { return createAttributeNsFor(this, namespace, qualifiedName); }
        importNode(node, deep = false) {
            if (!(node instanceof Node)) throw new TypeError('importNode requires a Node');
            const imported = wrap(host('importNode', this.__id, node.__id, !!deep));
            if (!imported) throw new DOMException('Documents cannot be imported', 'NotSupportedError');
            return imported;
        }
        createEvent(type) {
            const interfaceName = String(type).toLowerCase();
            const event = interfaceName === 'customevent' ? new CustomEvent('') :
                interfaceName === 'messageevent' ? new MessageEvent('') : new Event('');
            event.__initialized = false;
            return event;
        }
        getElementById(id) { return wrap(host('byId', this.__id, String(id))); }
        getElementsByTagName(name) { return this.querySelectorAll(String(name)); }
        getElementsByClassName(name) { return this.querySelectorAll('.' + String(name).trim().replace(/\s+/g, '.')); }
        getElementsByName(name) { return this.querySelectorAll('[name="' + String(name).replace(/"/g, '\\"') + '"]'); }
        get documentElement() { return this.children[0] || null; }
        get doctype() { return wrap(host('doctype', this.__id)); }
        get head() { return this.querySelector('head'); }
        get body() { return this.querySelector('body'); }
        get title() { return this.querySelector('title')?.textContent || ''; }
        set title(value) {
            let title = this.querySelector('title');
            if (!title) { title = this.createElement('title'); (this.head || this.documentElement).appendChild(title); }
            title.textContent = String(value);
        }
        get URL() { return host('documentUrl'); }
        get documentURI() { return this.URL; }
        get baseURI() { return this.querySelector('base')?.href || this.URL; }
        get currentScript() { return this._currentScript; }
        get defaultView() { return host('isPrimaryDocument', this.__id) ? windowObject : null; }
        get implementation() { return this.__implementation ||= new DOMImplementation(); }
        __setCurrentScript(id) { this._currentScript = wrap(id); }
        __dispatchNodeEvent(id, type) {
            const target = wrap(id);
            if (target) target.dispatchEvent(markTrusted(new Event(String(type))));
        }
        write(...parts) {
            host('documentWrite', parts.join(''));
            if (!documentWriteRefreshQueued) {
                documentWriteRefreshQueued = true;
                Promise.resolve().then(() => {
                    documentWriteRefreshQueued = false;
                    upgradeCustomElementTree(document);
                    refreshWindowNamedProperties();
                });
            }
        }
        writeln(...parts) { this.write(parts.join('') + '\n'); }
        hasFocus() { return true; }
        get hidden() { return false; }
        get visibilityState() { return 'visible'; }
        get compatMode() { return 'CSS1Compat'; }
        get characterSet() { return host('documentCharacterSet', this.__id); }
        get contentType() { return 'text/html'; }
        get cookie() { return host('cookieGet'); }
        set cookie(value) { host('cookieSet', String(value)); }
    }
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
                ? htmlElementConstructor(metadata[2].toLowerCase())
                : Element;
            node = new Constructor(id, type, metadata[1], metadata[2] || null, namespace);
        }
        else if (type === 10) node = new DocumentType(id, type, metadata[1], null, null);
        else if (type === 11) node = new DocumentFragment(id, type, metadata[1], null, null);
        else node = type === 8
            ? new Comment(id, type, metadata[1], null, null)
            : new Text(id, type, metadata[1], null, null);
        cache.set(id, node);
        return node;
    }

    const document = wrap(host('document'));
    const windowEvents = new EventTarget();
    const windowObject = globalThis;
    windowObject.window = windowObject;
    windowObject.self = windowObject;
    windowObject.top = windowObject;
    windowObject.parent = windowObject;
    windowObject.document = document;
    windowObject.Node = Node;
    windowObject.Element = Element;
    windowObject.Attr = Attr;
    windowObject.NamedNodeMap = NamedNodeMap;
    windowObject.HTMLElement = HTMLElement;
    windowObject.HTMLDivElement = HTMLDivElement;
    windowObject.HTMLUnknownElement = HTMLUnknownElement;
    windowObject.HTMLTimeElement = HTMLTimeElement;
    windowObject.HTMLDataElement = HTMLDataElement;
    windowObject.HTMLAnchorElement = HTMLAnchorElement;
    windowObject.HTMLDetailsElement = HTMLDetailsElement;
    windowObject.HTMLDialogElement = HTMLDialogElement;
    windowObject.HTMLScriptElement = HTMLScriptElement;
    windowObject.HTMLImageElement = HTMLImageElement;
    windowObject.HTMLPictureElement = HTMLPictureElement;
    windowObject.HTMLSourceElement = HTMLSourceElement;
    windowObject.HTMLInputElement = HTMLInputElement;
    windowObject.HTMLTextAreaElement = HTMLTextAreaElement;
    windowObject.HTMLOrderedListElement = HTMLOrderedListElement;
    windowObject.HTMLSelectElement = HTMLSelectElement;
    windowObject.HTMLButtonElement = HTMLButtonElement;
    windowObject.HTMLLabelElement = HTMLLabelElement;
    windowObject.HTMLFieldSetElement = HTMLFieldSetElement;
    windowObject.HTMLDataListElement = HTMLDataListElement;
    windowObject.HTMLOutputElement = HTMLOutputElement;
    windowObject.HTMLProgressElement = HTMLProgressElement;
    windowObject.HTMLMeterElement = HTMLMeterElement;
    windowObject.HTMLTemplateElement = HTMLTemplateElement;
    windowObject.HTMLFormElement = HTMLFormElement;
    windowObject.Document = Document;
    windowObject.Text = Text;
    windowObject.Comment = Comment;
    windowObject.DocumentType = DocumentType;
    windowObject.DocumentFragment = DocumentFragment;
    windowObject.DOMImplementation = DOMImplementation;
    windowObject.Event = Event;
    windowObject.CustomEvent = CustomEvent;
    windowObject.MessageEvent = MessageEvent;
    windowObject.ErrorEvent = ErrorEvent;
    windowObject.ToggleEvent = ToggleEvent;
    windowObject.UIEvent = UIEvent;
    windowObject.FocusEvent = FocusEvent;
    windowObject.MouseEvent = MouseEvent;
    windowObject.PointerEvent = PointerEvent;
    windowObject.KeyboardEvent = KeyboardEvent;
    windowObject.InputEvent = InputEvent;
    windowObject.DOMException = DOMException;
    windowObject.EventTarget = EventTarget;
    windowObject.DOMTokenList = DOMTokenList;
    windowObject.DOMStringMap = DOMStringMap;
    windowObject.CSSStyleDeclaration = CSSStyleDeclaration;
    installEventTargetProxy(windowEvents, windowObject);
    windowObject.addEventListener = windowEvents.addEventListener.bind(windowEvents);
    windowObject.removeEventListener = windowEvents.removeEventListener.bind(windowEvents);
    windowObject.dispatchEvent = windowEvents.dispatchEvent.bind(windowEvents);

    const installedWindowNames = new Map();
    refreshWindowNamedProperties = () => {
        const names = new Set(JSON.parse(host('namedPropertyNames')));
        for (const [name, getter] of installedWindowNames) {
            if (names.has(name)) continue;
            if (Object.getOwnPropertyDescriptor(windowObject, name)?.get === getter) delete windowObject[name];
            installedWindowNames.delete(name);
        }
        for (const name of names) {
            if (name in windowObject) continue;
            const getter = () => {
                const objects = list(host('namedProperty', name));
                return objects.length > 1 ? objects : objects[0];
            };
            const setter = value => {
                Object.defineProperty(windowObject, name, {
                    configurable: true, enumerable: true, writable: true, value
                });
                installedWindowNames.delete(name);
            };
            Object.defineProperty(windowObject, name, {
                configurable: true, enumerable: true, get: getter, set: setter
            });
            installedWindowNames.set(name, getter);
        }
    };

    const iframeWindow = isolatedIframeWindow || windowObject;
    const iframeEvents = new EventTarget();
    installEventTargetProxy(iframeEvents, iframeWindow);
    iframeWindow.addEventListener = iframeEvents.addEventListener.bind(iframeEvents);
    iframeWindow.removeEventListener = iframeEvents.removeEventListener.bind(iframeEvents);
    iframeWindow.dispatchEvent = iframeEvents.dispatchEvent.bind(iframeEvents);
    const iframeDocument = {
        defaultView: iframeWindow,
        readyState: 'complete',
        URL: 'about:blank',
        documentURI: 'about:blank',
        baseURI: 'about:blank',
        createElement: name => document.createElement(name),
        createElementNS: (_namespace, name) => document.createElement(name),
        createAttribute: name => document.createAttribute(name),
        createAttributeNS: (namespace, name) => document.createAttributeNS(namespace, name),
        createTextNode: text => document.createTextNode(text),
        querySelector: selector => document.querySelector(selector),
        querySelectorAll: selector => document.querySelectorAll(selector)
    };
    iframeWindow.parent = windowObject;
    iframeWindow.top = windowObject;
    iframeWindow.document = iframeDocument;

    let currentUrl = host('documentUrl');
    const parseUrl = value => {
        const match = String(value).match(/^([a-z]+:)?\/\/([^/?#]+)?([^?#]*)?(\?[^#]*)?(#.*)?$/i);
        return {
            protocol: match?.[1] || '',
            host: match?.[2] || '',
            hostname: (match?.[2] || '').split(':')[0],
            pathname: match?.[3] || '/',
            search: match?.[4] || '',
            hash: match?.[5] || ''
        };
    };
    const location = {
        get href() { return currentUrl; },
        set href(value) { currentUrl = host('navigate', String(value)); },
        assign(value) { this.href = value; },
        replace(value) { this.href = value; },
        reload() { host('navigate', currentUrl); },
        toString() { return currentUrl; },
        get protocol() { return parseUrl(currentUrl).protocol; },
        get host() { return parseUrl(currentUrl).host; },
        get hostname() { return parseUrl(currentUrl).hostname; },
        get pathname() { return parseUrl(currentUrl).pathname; },
        get search() { return parseUrl(currentUrl).search; },
        get hash() { return parseUrl(currentUrl).hash; },
        get origin() { const parsed = parseUrl(currentUrl); return parsed.protocol + '//' + parsed.host; }
    };
    windowObject.location = location;
    document.location = location;
    function cloneMessageValue(value, memory = new Map()) {
        if (value === null || ['undefined', 'boolean', 'number', 'string', 'bigint'].includes(typeof value)) return value;
        if (typeof value === 'symbol' || typeof value === 'function') {
            throw new DOMException('The value could not be cloned', 'DataCloneError');
        }
        if (memory.has(value)) return memory.get(value);
        if (typeof ArrayBuffer !== 'undefined' && value instanceof ArrayBuffer) return value.slice(0);
        if (typeof ArrayBuffer !== 'undefined' && ArrayBuffer.isView?.(value)) {
            const buffer = value.buffer.slice(value.byteOffset, value.byteOffset + value.byteLength);
            return typeof DataView !== 'undefined' && value instanceof DataView
                ? new DataView(buffer)
                : new value.constructor(buffer);
        }
        if (value instanceof Date) return new Date(value.getTime());
        if (value instanceof RegExp) return new RegExp(value.source, value.flags);
        if (value instanceof Map) {
            const clone = new Map();
            memory.set(value, clone);
            for (const [key, entry] of value) clone.set(cloneMessageValue(key, memory), cloneMessageValue(entry, memory));
            return clone;
        }
        if (value instanceof Set) {
            const clone = new Set();
            memory.set(value, clone);
            for (const entry of value) clone.add(cloneMessageValue(entry, memory));
            return clone;
        }
        const prototype = Object.getPrototypeOf(value);
        if (prototype !== Object.prototype && prototype !== null && !Array.isArray(value)) {
            throw new DOMException('The value could not be cloned', 'DataCloneError');
        }
        const clone = Array.isArray(value) ? [] : {};
        memory.set(value, clone);
        for (const key of Object.keys(value)) clone[key] = cloneMessageValue(value[key], memory);
        return clone;
    }
    const targetOriginValue = targetOrigin => {
        targetOrigin = targetOrigin === undefined ? '/' : String(targetOrigin);
        if (targetOrigin === '*' || targetOrigin === '/') return targetOrigin;
        const parsed = parseUrl(host('resolveUrl', targetOrigin));
        if (!parsed.protocol || !parsed.host) throw new DOMException('Invalid target origin', 'SyntaxError');
        return parsed.protocol + '//' + parsed.host;
    };
    function postMessageTo(targetEvents, message, targetOriginOrOptions = '/', transfer = []) {
        let targetOrigin = targetOriginOrOptions;
        if (targetOriginOrOptions && typeof targetOriginOrOptions === 'object') {
            targetOrigin = targetOriginOrOptions.targetOrigin ?? '/';
            transfer = targetOriginOrOptions.transfer || [];
        }
        if (transfer && transfer.length) {
            throw new DOMException('Transferable objects are not implemented', 'DataCloneError');
        }
        const cloned = cloneMessageValue(message);
        const expectedOrigin = targetOriginValue(targetOrigin);
        if (expectedOrigin !== '*' && expectedOrigin !== '/' && expectedOrigin !== location.origin) return;
        setTimeout(() => targetEvents.dispatchEvent(markTrusted(new MessageEvent('message', {
            data: cloned,
            origin: location.origin,
            source: windowObject,
            ports: []
        }))), 0);
    }
    windowObject.postMessage = (message, targetOriginOrOptions = '/', transfer = []) =>
        postMessageTo(windowEvents, message, targetOriginOrOptions, transfer);
    iframeWindow.postMessage = (message, targetOriginOrOptions = '/', transfer = []) =>
        postMessageTo(iframeEvents, message, targetOriginOrOptions, transfer);
