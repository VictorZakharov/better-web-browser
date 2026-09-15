    // HTML's handler map is distinct from the DOM listener list. Compilation is lazy, but
    // activation reserves a listener position immediately, even for syntactically invalid text.
    // https://html.spec.whatwg.org/multipage/webappapis.html#event-handlers
    const eventHandlerStore = new WeakMap();
    const eventHandlerAttributeValues = new WeakMap();
    const eventHandlerTypes = (
        'abort animationcancel animationend animationiteration animationstart auxclick beforeinput beforematch beforetoggle blur cancel canplay canplaythrough change ' +
        'click close command contextlost contextmenu contextrestored copy cuechange cut dblclick drag ' +
        'dragend dragenter dragleave dragover dragstart drop durationchange emptied encrypted ended error focus freeze ' +
        'formdata input invalid keydown keypress keyup load loadeddata loadedmetadata loadstart mousedown ' +
        'mouseenter mouseleave mousemove mouseout mouseover mouseup pointerover pointerenter pointerdown ' +
        'pointermove pointerup pointercancel pointerout pointerleave gotpointercapture lostpointercapture ' +
        'paste pause play playing progress ratechange ' +
        'readystatechange reset resize resume scroll scrollend securitypolicyviolation seeked seeking select slotchange stalled submit ' +
        'suspend timeupdate toggle transitioncancel transitionend transitionrun transitionstart unload visibilitychange volumechange waiting wheel message ' +
        'webkitanimationend webkitanimationiteration webkitanimationstart webkittransitionend'
    ).split(/\s+/);
    const windowHandlerTypes = (
        'afterprint beforeprint beforeunload hashchange languagechange message messageerror offline online ' +
        'pagehide pagereveal pageshow pageswap popstate rejectionhandled storage unhandledrejection unload'
    ).split(/\s+/);
    const reflectedBodyHandlerTypes = new Set([
        ...windowHandlerTypes, 'blur', 'error', 'focus', 'load', 'resize', 'scroll'
    ]);
    const handlerEventType = type => ({
        webkitanimationend: 'webkitAnimationEnd', webkitanimationiteration: 'webkitAnimationIteration',
        webkitanimationstart: 'webkitAnimationStart', webkittransitionend: 'webkitTransitionEnd'
    })[type] || type;
    const contentHandlerTypes = new Set(eventHandlerTypes.filter(type =>
        !['readystatechange', 'freeze', 'resume', 'unload', 'visibilitychange', 'message'].includes(type)));
    const isBodyEventTarget = target => target instanceof HTMLElement &&
        (target.localName === 'body' || target.localName === 'frameset');
    const eventHandlerTarget = (target, type) => {
        target = receiverFor(storageFor(target));
        if (isBodyEventTarget(target) && reflectedBodyHandlerTypes.has(type))
            target = target.ownerDocument.defaultView;
        return target === null ? null : storageFor(target);
    };
    const handlerDocument = target => {
        const receiver = receiverFor(target);
        return receiver instanceof Node ? receiver.ownerDocument : receiver.document;
    };
    const getEventHandler = (target, type) => {
        if (target === null) return null;
        const handler = eventHandlerStore.get(target)?.get(type);
        if (!handler) return null;
        if (handler.raw !== null) {
            const receiver = receiverFor(target);
            const doc = handlerDocument(target);
            // Inert document handlers retain their uncompiled source for later adoption.
            if (!doc?.defaultView) return null;
            const scopes = [];
            if (receiver instanceof Element) {
                scopes.push(doc);
                if (receiver instanceof HTMLElement &&
                    ['button', 'fieldset', 'input', 'object', 'output', 'select', 'textarea', 'img'].includes(receiver.localName)) {
                    const form = associatedForm(receiver);
                    if (form) scopes.push(form);
                }
                scopes.push(receiver);
            }
            const location = handler.raw;
            handler.raw = null;
            try {
                handler.value = host('compileEventHandler', location.body, 'on' + type, scopes,
                    location.url, doc.defaultView, type === 'error' && !(receiver instanceof Node));
            } catch (error) {
                // A compilation failure clears the value, not the activated listener.
                handler.value = null;
                reportGlobalException(error, 'event handler', doc.defaultView, location.url);
            }
        }
        return handler.value;
    };
    const activateEventHandler = (target, type, handler) => {
        if (handler.listener) return;
        handler.listener = addListener(target, handlerEventType(type), function(event) {
            const callback = getEventHandler(target, type);
            if (typeof callback !== 'function') return;
            const specialError = event instanceof ErrorEvent && type === 'error' &&
                proxyStorage.has(this);
            const args = specialError
                ? [event.message, event.filename, event.lineno, event.colno, event.error] : [event];
            let result = Reflect.apply(callback, this, args);
            if (type === 'beforeunload') {
                // The nullable DOMString callback return conversion also applies when an
                // ordinary Event (rather than a BeforeUnloadEvent) is dispatched.
                result = result == null ? null : '' + result;
                if (event instanceof BeforeUnloadEvent && result !== null) {
                    event.__canceled = true;
                    if (event.returnValue === '') event.returnValue = result;
                    return;
                }
            }
            if (specialError ? result === true : result === false) event.preventDefault();
        }, false);
    };
    const setEventHandler = (target, type, value, raw = null) => {
        if (target === null) return;
        let handlers = eventHandlerStore.get(target);
        let handler = handlers?.get(type);
        if (value === null && raw === null) {
            if (handler) {
                handler.value = null;
                handler.raw = null;
                if (handler.listener) removeListener(target, handler.listener);
                handlers.delete(type);
            }
            return;
        }
        if (!handlers) eventHandlerStore.set(target, handlers = new Map());
        if (!handler) {
            handler = { value: null, raw: null, listener: null };
            handlers.set(type, handler);
        }
        handler.value = value;
        handler.raw = raw;
        activateEventHandler(target, type, handler);
    };
    const defineEventHandler = (receiver, storage, type) => {
        const name = 'on' + type;
        if (Object.getOwnPropertyDescriptor(receiver, name)?.configurable === false) return;
        Object.defineProperty(receiver, name, {
            configurable: true, enumerable: true,
            get() { return getEventHandler(eventHandlerTarget(storage || this, type), type); },
            set(value) {
                // Web IDL's [LegacyTreatNonObjectAsNull] preserves non-callable objects;
                // processing ignores them without consulting a handleEvent property.
                if (typeof value !== 'function' && (typeof value !== 'object' || value === null)) value = null;
                setEventHandler(eventHandlerTarget(storage || this, type), type, value);
            }
        });
    };
    const installEventHandlerAttributes = receiver => {
        for (const type of eventHandlerTypes) defineEventHandler(receiver, null, type);
    };
    const installEventTargetProxy = (storage, proxy) => {
        proxyStorage.set(proxy, storage);
        storageProxy.set(storage, proxy);
        for (const type of new Set([...eventHandlerTypes, ...windowHandlerTypes]))
            defineEventHandler(proxy, storage, type);
    };
    const eventHandlerAttributeChanged = (element, record, value) => {
        if (record.namespace !== null || !record.localName.startsWith('on') ||
            !(element instanceof HTMLElement || element instanceof SVGElement)) return;
        const type = record.localName.slice(2);
        if (!contentHandlerTypes.has(type) && !(isBodyEventTarget(element) && reflectedBodyHandlerTypes.has(type))) return;
        let values = eventHandlerAttributeValues.get(element);
        if (!values) eventHandlerAttributeValues.set(element, values = new Map());
        if (value === null) values.delete(record.localName);
        else values.set(record.localName, value);
        const raw = value === null ? null : { body: value, url: element.ownerDocument.URL };
        setEventHandler(eventHandlerTarget(element, type), type, null, raw);
    };
    const initializeEventHandlerAttributes = element => {
        if (!(element instanceof Element)) return;
        for (const record of attributeRecords(element))
            eventHandlerAttributeChanged(element, record, record.value);
    };
    const refreshParserEventHandlerAttributes = () => {
        // HTML's in-body parser can merge missing attributes into existing html/body nodes.
        // These are not new wrappers. Compare content, not IDL state: an unchanged attribute
        // must not reactivate a handler that author code cleared or replaced.
        for (const element of [document.documentElement, document.body]) {
            if (!element) continue;
            const previous = eventHandlerAttributeValues.get(element);
            for (const record of attributeRecords(element)) {
                if (record.namespace === null && record.value !== previous?.get(record.localName))
                    eventHandlerAttributeChanged(element, record, record.value);
            }
        }
    };
