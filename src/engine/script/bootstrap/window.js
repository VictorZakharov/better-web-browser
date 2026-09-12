    // Window exposure, named access, iframe views, and window messaging.
    const windowEvents = new EventTarget();
    const windowObject = globalThis;
    const windowConstructionToken = {};
    class Window extends EventTarget {
        constructor(token) {
            if (token !== windowConstructionToken) throw new TypeError('Illegal constructor');
            super();
        }
    }
    Object.setPrototypeOf(windowObject, Window.prototype);
    windowObject.window = windowObject;
    windowObject.self = windowObject;
    windowObject.top = windowObject;
    windowObject.parent = windowObject;
    windowObject.document = document;
    windowObject.Node = Node;
    windowObject.Element = Element;
    windowObject.SVGAnimatedString = SVGAnimatedString;
    windowObject.SVGElement = SVGElement;
    windowObject.SVGSVGElement = SVGSVGElement;
    windowObject.Attr = Attr;
    windowObject.NamedNodeMap = NamedNodeMap;
    windowObject.NodeFilter = NodeFilter;
    windowObject.TreeWalker = TreeWalker;
    windowObject.HTMLElement = HTMLElement;
    windowObject.HTMLDivElement = HTMLDivElement;
    windowObject.HTMLTitleElement = HTMLTitleElement;
    windowObject.HTMLStyleElement = HTMLStyleElement;
    windowObject.HTMLLinkElement = HTMLLinkElement;
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
    windowObject.HTMLIFrameElement = HTMLIFrameElement;
    windowObject.HTMLMediaElement = HTMLMediaElement;
    windowObject.HTMLVideoElement = HTMLVideoElement;
    windowObject.HTMLAudioElement = HTMLAudioElement;
    windowObject.HTMLCanvasElement = HTMLCanvasElement;
    windowObject.CanvasRenderingContext2D = CanvasRenderingContext2D;
    windowObject.ImageData = ImageData;
    windowObject.TimeRanges = TimeRanges;
    windowObject.MediaError = MediaError;
    windowObject.MediaSource = MediaSource;
    windowObject.SourceBuffer = SourceBuffer;
    windowObject.SourceBufferList = SourceBufferList;
    windowObject.HTMLInputElement = HTMLInputElement;
    windowObject.HTMLTextAreaElement = HTMLTextAreaElement;
    windowObject.HTMLOrderedListElement = HTMLOrderedListElement;
    windowObject.HTMLSelectElement = HTMLSelectElement;
    windowObject.HTMLOptionElement = HTMLOptionElement;
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
    windowObject.CharacterData = CharacterData;
    windowObject.Text = Text;
    windowObject.CDATASection = CDATASection;
    windowObject.Comment = Comment;
    windowObject.ProcessingInstruction = ProcessingInstruction;
    windowObject.DocumentType = DocumentType;
    windowObject.DocumentFragment = DocumentFragment;
    windowObject.ShadowRoot = ShadowRoot;
    windowObject.HTMLSlotElement = HTMLSlotElement;
    windowObject.DOMImplementation = DOMImplementation;
    windowObject.HTMLCollection = HTMLCollection;
    windowObject.Event = Event;
    windowObject.CustomEvent = CustomEvent;
    windowObject.MessageEvent = MessageEvent;
    windowObject.ErrorEvent = ErrorEvent;
    windowObject.ToggleEvent = ToggleEvent;
    windowObject.UIEvent = UIEvent;
    windowObject.FocusEvent = FocusEvent;
    windowObject.MouseEvent = MouseEvent;
    windowObject.PointerEvent = PointerEvent;
    windowObject.WheelEvent = WheelEvent;
    windowObject.KeyboardEvent = KeyboardEvent;
    windowObject.InputEvent = InputEvent;
    windowObject.DOMException = DOMException;
    windowObject.EventTarget = EventTarget;
    windowObject.Window = Window;
    windowObject.Audio = function Audio(src = '') {
        const audio = document.createElement('audio');
        if (src !== '') audio.src = String(src);
        return audio;
    };
    windowObject.DOMTokenList = DOMTokenList;
    windowObject.DOMStringMap = DOMStringMap;
    windowObject.CSSStyleDeclaration = CSSStyleDeclaration;
    installEventTargetProxy(windowEvents, windowObject);
    windowObject.addEventListener = windowEvents.addEventListener.bind(windowEvents);
    windowObject.removeEventListener = windowEvents.removeEventListener.bind(windowEvents);
    windowObject.dispatchEvent = windowEvents.dispatchEvent.bind(windowEvents);
    const installedWindowNames = new Map();
    const synchronizeWindowName = name => {
        name = String(name || '');
        if (!name) return;
        const objects = list(host('namedProperty', name));
        const installedGetter = installedWindowNames.get(name);
        if (!objects.length) {
            if (installedGetter && Object.getOwnPropertyDescriptor(windowObject, name)?.get === installedGetter)
                delete windowObject[name];
            installedWindowNames.delete(name);
            return;
        }
        if (installedGetter || name in windowObject) return;
        const getter = () => {
            const current = list(host('namedProperty', name));
            return current.length > 1 ? current : current[0];
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
    };
    refreshWindowNamedPropertyValues = values => {
        for (const name of new Set(values)) synchronizeWindowName(name);
    };
    refreshWindowNamedProperties = roots => {
        if (roots !== undefined) {
            roots = Array.isArray(roots) ? roots : [roots];
            const names = new Set();
            for (const root of roots) {
                if (!(root instanceof Node)) continue;
                for (const name of JSON.parse(host('namedPropertyCandidates', root.__id))) names.add(name);
            }
            refreshWindowNamedPropertyValues(names);
            return;
        }
        const names = new Set(JSON.parse(host('namedPropertyNames')));
        for (const name of installedWindowNames.keys()) if (!names.has(name)) synchronizeWindowName(name);
        for (const name of names) {
            synchronizeWindowName(name);
        }
    };

    const iframeWindow = isolatedIframeWindow || windowObject;
    const iframeEvents = new EventTarget();
    installEventTargetProxy(iframeEvents, iframeWindow);
    iframeWindow.addEventListener = iframeEvents.addEventListener.bind(iframeEvents);
    iframeWindow.removeEventListener = iframeEvents.removeEventListener.bind(iframeEvents);
    iframeWindow.dispatchEvent = iframeEvents.dispatchEvent.bind(iframeEvents);
    const iframeDocuments = new WeakMap();
    const iframeDocumentFor = frame => {
        let iframeDocument = iframeDocuments.get(frame);
        if (!iframeDocument && frame.isConnected) {
            // A same-origin about:blank iframe owns a real HTML document. Its descendants keep
            // that document as their root even if the embedding element is later disconnected.
            // https://html.spec.whatwg.org/multipage/iframe-embed-object.html#the-iframe-element
            iframeDocument = wrap(host('createHtmlDocument', ''));
            documentDefaultViews.set(iframeDocument, iframeWindow);
            iframeDocuments.set(frame, iframeDocument);
        }
        if (iframeDocument) iframeWindow.document = iframeDocument;
        return iframeDocument || null;
    };
    iframeWindow.parent = windowObject;
    iframeWindow.top = windowObject;
    iframeWindow.document = null;

    let currentUrl = host('documentUrl');
    const parseUrl = value => JSON.parse(host('parseWebUrl', String(value)));
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
