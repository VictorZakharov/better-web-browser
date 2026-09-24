    // Window exposure, named access, iframe views, and window messaging.
    const windowEvents = new EventTarget();
    const windowObject = globalThis;
    const frameElementObject = globalThis.__frameElement || null;
    delete globalThis.__frameElement;
    Object.defineProperties(windowObject, {
        name: { configurable: true, enumerable: true,
            get: () => host('windowName'),
            set: value => host('setWindowName', String(value)) },
        length: { configurable: true, enumerable: true,
            get: () => document.querySelectorAll('iframe').length },
        frameElement: { configurable: true, enumerable: true,
            get: () => host('frameActive') ? frameElementObject : null },
        closed: { configurable: true, enumerable: true, get: () => !host('frameActive') }
    });
    Object.assign(windowObject, { HTMLBodyElement, HTMLFrameSetElement, BeforeUnloadEvent });
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
    windowObject.frames = windowObject;
    windowObject.top = windowObject;
    windowObject.parent = windowObject;
    windowObject.document = document;
    windowObject.Node = Node;
    windowObject.Element = Element;
    windowObject.SVGAnimatedString = SVGAnimatedString;
    windowObject.SVGElement = SVGElement;
    windowObject.SVGSVGElement = SVGSVGElement;
    windowObject.SVGAnimatedEnumeration = SVGAnimatedEnumeration;
    windowObject.SVGNumber = SVGNumber;
    windowObject.SVGNumberList = SVGNumberList;
    windowObject.SVGAnimatedNumberList = SVGAnimatedNumberList;
    windowObject.SVGFEColorMatrixElement = SVGFEColorMatrixElement;
    windowObject.SVGLength = SVGLength;
    windowObject.SVGAnimatedLength = SVGAnimatedLength;
    windowObject.SVGUnitTypes = SVGUnitTypes;
    windowObject.SVGFilterElement = SVGFilterElement;
    Object.assign(windowObject, {
        SVGFEOffsetElement, SVGFEGaussianBlurElement, SVGFECompositeElement,
        SVGFEBlendElement, SVGFEFloodElement, SVGFEMergeElement, SVGFEMergeNodeElement
    });
    windowObject.SVGAnimatedNumber = SVGAnimatedNumber;
    windowObject.Attr = Attr;
    windowObject.NamedNodeMap = NamedNodeMap;
    windowObject.NodeFilter = NodeFilter;
    windowObject.TreeWalker = TreeWalker;
    windowObject.NodeIterator = NodeIterator;
    windowObject.HTMLElement = HTMLElement;
    windowObject.HTMLDivElement = HTMLDivElement;
    windowObject.HTMLHtmlElement = HTMLHtmlElement;
    windowObject.HTMLParagraphElement = HTMLParagraphElement;
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
    windowObject.OffscreenCanvasRenderingContext2D = OffscreenCanvasRenderingContext2D;
    windowObject.OffscreenCanvas = OffscreenCanvas;
    windowObject.ImageBitmap = ImageBitmap;
    windowObject.ImageBitmapRenderingContext = ImageBitmapRenderingContext;
    windowObject.Path2D = Path2D;
    windowObject.CanvasGradient = CanvasGradient;
    windowObject.CanvasPattern = CanvasPattern;
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
    windowObject.ValidityState = ValidityState;
    windowObject.Document = Document;
    windowObject.HTMLDocument = Document;
    windowObject.XMLDocument = XMLDocument;
    windowObject.DOMParser = DOMParser;
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
    windowObject.DragEvent = DragEvent;
    windowObject.ClipboardEvent = ClipboardEvent;
    windowObject.DataTransfer = DataTransfer;
    windowObject.DataTransferItem = DataTransferItem;
    windowObject.DataTransferItemList = DataTransferItemList;
    windowObject.FileList = FileList;
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
            if (current.length === 1 && current[0]?.localName === 'iframe')
                return current[0].contentWindow;
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
                if (!(isNode(root))) continue;
                for (const name of JSON.parse(host('namedPropertyCandidates', nodeId(root)))) names.add(name);
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


    let currentUrl = host('documentUrl');
    const parseUrl = value => JSON.parse(host('parseWebUrl', String(value)));
    const location = {
        get href() { return currentUrl; },
        set href(value) { navigateLocation(value, false); },
        assign(value) { this.href = value; },
        replace(value) { navigateLocation(value, true); },
        reload() { host('navigate', currentUrl, true); },
        toString() { return currentUrl; },
        get protocol() { return parseUrl(currentUrl).protocol; },
        get host() { return parseUrl(currentUrl).host; },
        get hostname() { return parseUrl(currentUrl).hostname; },
        get pathname() { return parseUrl(currentUrl).pathname; },
        get search() { return parseUrl(currentUrl).search; },
        get hash() { return parseUrl(currentUrl).hash; },
        set hash(value) {
            const target = host('setWebUrlComponent', currentUrl, 'hash', String(value) || '#');
            if (target !== currentUrl) navigateLocation(target, false);
        },
        get origin() { return parseUrl(currentUrl).origin; }
    };
    windowObject.location = location;
    Object.defineProperty(windowObject, 'origin', { configurable: true, enumerable: true,
        get: () => host('documentOrigin') });
    const MessageEventConstructor = MessageEvent;
    const MessageDOMException = DOMException;
    windowObject.__windowMessageBindings = [
        (targetOrigin = '/', transfer = []) => {
            if (targetOrigin !== null && typeof targetOrigin === 'object') {
                transfer = targetOrigin.transfer ?? [];
                targetOrigin = targetOrigin.targetOrigin ?? '/';
            }
            if (typeof targetOrigin === 'symbol') throw new TypeError('Cannot convert a Symbol to a string');
            if (transfer == null || typeof transfer[Symbol.iterator] !== 'function')
                throw new TypeError('The transfer list must be iterable');
            return [String(targetOrigin), [...transfer]];
        },
        (data, origin, source, ports) => windowEvents.dispatchEvent(markTrusted(new MessageEventConstructor('message', {
            data, origin, source, ports
        }))),
        (message, name) => new MessageDOMException(message, name)
    ];
