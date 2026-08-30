    const trustedEvents = new WeakSet();
    const markTrusted = event => { trustedEvents.add(event); return event; };
    Object.defineProperty(globalThis, '__markTrustedEvent', {
        value: markTrusted, configurable: true,
    });

    class Event {
        constructor(type, init = {}) {
            init = init == null ? {} : Object(init);
            this.__type = String(type);
            this.__bubbles = !!init.bubbles;
            this.__cancelable = !!init.cancelable;
            this.__composed = !!init.composed;
            this.__target = null;
            this.__currentTarget = null;
            this.__phase = Event.NONE;
            this.__path = [];
            this.__stopped = false;
            this.__immediate = false;
            this.__canceled = false;
            this.__passive = false;
            this.__dispatching = false;
            this.__initialized = true;
            this.timeStamp = Date.now();
        }
        get type() { return this.__type; }
        get target() { return this.__target; }
        get srcElement() { return this.__target; }
        get currentTarget() { return this.__currentTarget; }
        get eventPhase() { return this.__phase; }
        get bubbles() { return this.__bubbles; }
        get cancelable() { return this.__cancelable; }
        get composed() { return this.__composed; }
        get defaultPrevented() { return this.__canceled; }
        get isTrusted() { return trustedEvents.has(this); }
        get cancelBubble() { return this.__stopped; }
        set cancelBubble(value) { if (value) this.stopPropagation(); }
        get returnValue() { return !this.__canceled; }
        set returnValue(value) { if (!value) this.preventDefault(); }
        composedPath() { return [...this.__path]; }
        preventDefault() {
            if (this.__cancelable && !this.__passive) this.__canceled = true;
        }
        stopPropagation() { this.__stopped = true; }
        stopImmediatePropagation() {
            this.__stopped = true;
            this.__immediate = true;
        }
        initEvent(type, bubbles = false, cancelable = false) {
            if (this.__dispatching) return;
            this.__type = String(type);
            this.__bubbles = !!bubbles;
            this.__cancelable = !!cancelable;
            this.__target = null;
            this.__currentTarget = null;
            this.__phase = Event.NONE;
            this.__path = [];
            this.__stopped = false;
            this.__immediate = false;
            this.__canceled = false;
            this.__passive = false;
            this.__initialized = true;
        }
    }
    for (const [name, value] of Object.entries({
        NONE: 0,
        CAPTURING_PHASE: 1,
        AT_TARGET: 2,
        BUBBLING_PHASE: 3
    })) {
        Object.defineProperty(Event, name, { enumerable: true, value });
        Object.defineProperty(Event.prototype, name, { enumerable: true, value });
    }

    class CustomEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            this.detail = init?.detail === undefined ? null : init.detail;
        }
        initCustomEvent(type, bubbles = false, cancelable = false, detail = null) {
            if (this.__dispatching) return;
            this.initEvent(type, bubbles, cancelable);
            this.detail = detail;
        }
    }
    class MessageEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            init = init == null ? {} : Object(init);
            this.data = init.data === undefined ? null : init.data;
            this.origin = init.origin === undefined ? '' : String(init.origin);
            this.lastEventId = init.lastEventId === undefined ? '' : String(init.lastEventId);
            this.source = init.source === undefined ? null : init.source;
            this.ports = Object.freeze([...(init.ports || [])]);
        }
        initMessageEvent(type, bubbles = false, cancelable = false, data = null, origin = '', lastEventId = '', source = null, ports = []) {
            if (this.__dispatching) return;
            this.initEvent(type, bubbles, cancelable);
            this.data = data;
            this.origin = String(origin);
            this.lastEventId = String(lastEventId);
            this.source = source;
            this.ports = Object.freeze([...ports]);
        }
    }
    class ErrorEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            init = init == null ? {} : Object(init);
            this.message = init.message === undefined ? '' : String(init.message);
            this.filename = init.filename === undefined ? '' : String(init.filename);
            this.lineno = Number(init.lineno) || 0;
            this.colno = Number(init.colno) || 0;
            this.error = init.error === undefined ? null : init.error;
        }
    }
    class ToggleEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            this.oldState = init?.oldState === undefined ? '' : String(init.oldState);
            this.newState = init?.newState === undefined ? '' : String(init.newState);
            this.source = init?.source === undefined ? null : init.source;
        }
    }
    const listenerStore = new WeakMap();
    const eventHandlerStore = new WeakMap();
    const proxyStorage = new WeakMap();
    const storageProxy = new WeakMap();
    const eventHandlerTypes = (
        'abort auxclick beforeinput beforematch beforetoggle blur cancel canplay canplaythrough change ' +
        'click close command contextlost contextmenu contextrestored copy cuechange cut dblclick drag ' +
        'dragend dragenter dragleave dragover dragstart drop durationchange emptied encrypted ended error focus freeze ' +
        'formdata input invalid keydown keypress keyup load loadeddata loadedmetadata loadstart mousedown ' +
        'mouseenter mouseleave mousemove mouseout mouseover mouseup paste pause play playing progress ratechange ' +
        'readystatechange reset resize resume scroll scrollend securitypolicyviolation seeked seeking select slotchange stalled submit ' +
        'suspend timeupdate toggle unload visibilitychange volumechange waiting wheel message'
    ).split(/\s+/);

    const storageFor = target => proxyStorage.get(target) || target;
    const receiverFor = target => storageProxy.get(target) || target;
    const listenersFor = target => {
        let listeners = listenerStore.get(target);
        if (!listeners) listenerStore.set(target, listeners = []);
        return listeners;
    };
    const captureOption = options => typeof options === 'boolean' ? options :
        options == null ? false : !!Object(options).capture;
    const listenerOptions = options => {
        if (typeof options === 'boolean' || options == null)
            return { capture: captureOption(options), once: false, passive: false };
        options = Object(options);
        return { capture: !!options.capture, passive: !!options.passive, once: !!options.once };
    };
    const removeListener = (target, listener) => {
        listener.removed = true;
        const listeners = listenerStore.get(target);
        const index = listeners?.indexOf(listener) ?? -1;
        if (index >= 0) listeners.splice(index, 1);
    };
    const addListener = (target, type, callback, options) => {
        type = String(type);
        const flattened = listenerOptions(options);
        if (callback == null) return;
        const kind = typeof callback;
        if (kind !== 'function' && kind !== 'object') return;
        const listeners = listenersFor(target);
        if (listeners.some(listener => !listener.removed && listener.type === type &&
            listener.callback === callback && listener.capture === flattened.capture)) return;
        listeners.push({ type, callback, ...flattened, removed: false });
    };

    const reportListenerException = error => {
        const detail = error?.stack || error?.message || String(error);
        host('console', 'error', 'Uncaught event listener exception: ' + detail);
    };
    const invokeListeners = (receiver, event, phase, capture) => {
        if (event.__stopped) return;
        const storage = storageFor(receiver);
        event.__currentTarget = receiver;
        event.__phase = phase;
        // DOM's inner-invoke algorithm clones per invocation: additions wait, while removed records
        // remain visible and are skipped through their shared flag.
        // https://dom.spec.whatwg.org/#concept-event-listener-inner-invoke
        const snapshot = [...(listenerStore.get(storage) || [])];
        for (const listener of snapshot) {
            if (listener.removed || listener.type !== event.type || listener.capture !== capture) continue;
            if (listener.once) removeListener(storage, listener);
            event.__passive = listener.passive;
            try {
                if (typeof listener.callback === 'function') listener.callback.call(receiver, event);
                else {
                    const handleEvent = listener.callback.handleEvent;
                    if (typeof handleEvent === 'function') handleEvent.call(listener.callback, event);
                }
            } catch (error) {
                reportListenerException(error);
            } finally {
                event.__passive = false;
            }
            if (event.__immediate) break;
        }
    };
    const shadowIncludingContains = (ancestor, node) => {
        if (!(ancestor instanceof Node) || !(node instanceof Node)) return false;
        for (let current = node; current; current = current.parentNode ||
            (current instanceof ShadowRoot ? current.host : null)) {
            if (current === ancestor) return true;
        }
        return false;
    };
    const retarget = (target, against) => {
        let adjusted = target;
        while (adjusted instanceof Node) {
            const root = adjusted.getRootNode();
            if (!(root instanceof ShadowRoot) ||
                (against instanceof Node && shadowIncludingContains(root, against))) return adjusted;
            adjusted = root.host;
        }
        return adjusted;
    };
    const closedShadowHidden = (node, currentTarget) => {
        if (!(node instanceof Node)) return false;
        let root = node.getRootNode();
        while (root instanceof ShadowRoot) {
            if (root.mode === 'closed' && !shadowIncludingContains(root, currentTarget)) return true;
            root = root.host.getRootNode();
        }
        return false;
    };
    Event.prototype.composedPath = function() {
        return this.__path.filter(node => !closedShadowHidden(node, this.__currentTarget));
    };
    const eventParent = (target, event) => {
        if (!(target instanceof Node)) return null;
        if (target.assignedSlot) return target.assignedSlot;
        const parent = target.parentNode;
        if (parent) return parent;
        if (target instanceof ShadowRoot) {
            const targetRoot = event.__originalTarget instanceof Node
                ? event.__originalTarget.getRootNode()
                : null;
            return event.composed || targetRoot !== target ? target.host : null;
        }
        if (target.nodeType === 9 && event.type !== 'load') return target.defaultView;
        return null;
    };
    const eventPath = (target, event) => {
        const path = [target];
        const seen = new Set(path);
        for (let parent = eventParent(target, event); parent && !seen.has(parent); parent = eventParent(parent, event)) {
            path.push(parent);
            seen.add(parent);
        }
        return path;
    };

    const getEventHandler = (target, type) => eventHandlerStore.get(target)?.get(type)?.value || null;
    const setEventHandler = (target, type, value) => {
        value = typeof value === 'function' ? value : null;
        let handlers = eventHandlerStore.get(target);
        let handler = handlers?.get(type);
        if (!value) {
            if (handler) {
                handler.value = null;
                removeListener(target, handler.listener);
                handlers.delete(type);
            }
            return;
        }
        if (handler) {
            handler.value = value;
            return;
        }
        if (!handlers) eventHandlerStore.set(target, handlers = new Map());
        // HTML activates one non-capture listener slot. Replacing its callback must not move that
        // slot relative to addEventListener() registrations.
        // https://html.spec.whatwg.org/multipage/webappapis.html#event-handler-idl-attributes
        handler = { value, listener: null };
        handler.listener = function(event) {
            const result = handler.value?.call(this, event);
            if (result === false) event.preventDefault();
        };
        handlers.set(type, handler);
        addListener(target, type, handler.listener, false);
    };
    const defineEventHandler = (receiver, storage, type) => {
        const name = 'on' + type;
        if (Object.getOwnPropertyDescriptor(receiver, name)?.configurable === false) return;
        Object.defineProperty(receiver, name, {
            configurable: true,
            enumerable: true,
            get() { return getEventHandler(storage ? storageFor(storage) : storageFor(this), type); },
            set(value) { setEventHandler(storage ? storageFor(storage) : storageFor(this), type, value); }
        });
    };
    const installEventHandlerAttributes = receiver => {
        for (const type of eventHandlerTypes) defineEventHandler(receiver, null, type);
    };
    const installEventTargetProxy = (storage, proxy) => {
        proxyStorage.set(proxy, storage);
        storageProxy.set(storage, proxy);
        for (const type of eventHandlerTypes) defineEventHandler(proxy, storage, type);
    };

    class EventTarget {
        addEventListener(type, callback, options) {
            addListener(storageFor(this), type, callback, options);
        }
        removeEventListener(type, callback, options) {
            const target = storageFor(this);
            const capture = captureOption(options);
            const listener = listenerStore.get(target)?.find(candidate => !candidate.removed &&
                candidate.type === String(type) && candidate.callback === callback && candidate.capture === capture);
            if (listener) removeListener(target, listener);
        }
        dispatchEvent(event) {
            if (!(event instanceof Event)) throw new TypeError('dispatchEvent requires an Event');
            if (event.__dispatching || !event.__initialized)
                throw new DOMException('The event is already being dispatched or is not initialized', 'InvalidStateError');
            const target = receiverFor(storageFor(this));
            event.__dispatching = true;
            event.__originalTarget = target;
            event.__target = target;
            try {
                const path = eventPath(target, event);
                event.__path = [...path];
                for (let index = path.length - 1; index > 0; index--) {
                    event.__target = retarget(target, path[index]);
                    invokeListeners(path[index], event, Event.CAPTURING_PHASE, true);
                }
                // The target participates in both listener passes even when the event does not
                // bubble; both passes expose AT_TARGET.
                event.__target = target;
                invokeListeners(target, event, Event.AT_TARGET, true);
                invokeListeners(target, event, Event.AT_TARGET, false);
                if (event.bubbles) {
                    for (let index = 1; index < path.length; index++) {
                        event.__target = retarget(target, path[index]);
                        invokeListeners(path[index], event, Event.BUBBLING_PHASE, false);
                    }
                }
            } finally {
                event.__target = target;
                event.__phase = Event.NONE;
                event.__currentTarget = null;
                event.__path = [];
                event.__dispatching = false;
                event.__stopped = false;
                event.__immediate = false;
                event.__passive = false;
            }
            return !event.defaultPrevented;
        }
    }
