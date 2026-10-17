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
    let blockingWheelListeners = 0;
    Object.defineProperty(globalThis, '__hasBlockingWheelListeners', {
        value: () => blockingWheelListeners > 0,
    });
    const proxyStorage = new WeakMap();
    const storageProxy = new WeakMap();
    const legacyEventTargets = new WeakMap();
    let sensorListenerChanged = null;
    let broadcastListenerChanged = null;
    Object.defineProperty(globalThis, '__installBroadcastListenerHook', {
        configurable: true,
        value(callback) { broadcastListenerChanged = callback; }
    });
    Object.defineProperty(globalThis, '__installSensorListenerHook', {
        configurable: true,
        value(callback) { sensorListenerChanged = callback; }
    });
    let isNativeAbortSignal = () => false;
    Object.defineProperty(globalThis, '__installAbortSignalBrand', {
        configurable: true, value: checker => { isNativeAbortSignal = checker; }
    });

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
            return { capture: captureOption(options), once: false, passive: null, signal: undefined };
        options = Object(options);
        const capture = !!options.capture;
        const once = !!options.once;
        const passive = options.passive === undefined ? null : !!options.passive;
        const signal = options.signal;
        if (signal != null && !isNativeAbortSignal(signal))
            throw new TypeError('Event listener signal must be an AbortSignal');
        return { capture, once, passive, signal };
    };
    // Unspecified passive options on document-level wheel/touch targets cannot cancel scrolling,
    // so the browser may scroll before such listeners finish.
    // https://dom.spec.whatwg.org/#default-passive-value
    const passiveByDefaultTypes = new Set(['touchstart', 'touchmove', 'wheel', 'mousewheel']);
    const isHtmlElementNamed = (id, names) => host('nodeType', id) === 1 &&
        host('namespaceUri', id) === 'http://www.w3.org/1999/xhtml' &&
        names.includes(host('localName', id));
    const defaultPassiveValue = (type, target) => {
        if (!passiveByDefaultTypes.has(type)) return false;
        const receiver = receiverFor(target);
        if (receiver === globalThis) return true;
        if (!isNode(receiver)) return false;
        const id = nodeId(receiver);
        const parent = host('parent', id);
        if (host('nodeType', id) === 9) return true;
        if (!parent) return false;
        // The document element is the document's element child.
        if (host('nodeType', parent) === 9) return host('nodeType', id) === 1;
        const owner = host('parent', parent);
        if (!owner || host('nodeType', owner) !== 9 || !isHtmlElementNamed(parent, ['html']))
            return false;
        // The body element is the html element's first body or frameset child.
        // https://html.spec.whatwg.org/multipage/dom.html#the-body-element-2
        for (let child = host('firstChild', parent); child; child = host('nextSibling', child)) {
            if (isHtmlElementNamed(child, ['body', 'frameset'])) return child === id;
        }
        return false;
    };
    const removeListener = (target, listener) => {
        if (listener.removed) return;
        listener.removed = true;
        const listeners = listenerStore.get(target);
        const index = listeners?.indexOf(listener) ?? -1;
        if (index >= 0) listeners.splice(index, 1);
        if (index >= 0 && listener.type === 'wheel' && !listener.passive)
            blockingWheelListeners--;
        if (index >= 0 && sensorListenerChanged)
            sensorListenerChanged(receiverFor(target), listener.type, -1);
        if (index >= 0 && broadcastListenerChanged)
            broadcastListenerChanged(receiverFor(target), listener.type, -1);
        if (listener.abortRecord) removeListener(storageFor(listener.signal), listener.abortRecord);
    };
    const addListener = (target, type, callback, options) => {
        type = String(type);
        const flattened = listenerOptions(options);
        if (flattened.passive === null) flattened.passive = defaultPassiveValue(type, target);
        if (flattened.signal?.aborted) return;
        if (callback == null) return;
        const kind = typeof callback;
        if (kind !== 'function' && kind !== 'object') return;
        const listeners = listenersFor(target);
        if (listeners.some(listener => !listener.removed && listener.type === type &&
            listener.callback === callback && listener.capture === flattened.capture)) return;
        const listener = { type, callback, ...flattened, removed: false };
        listeners.push(listener);
        if (type === 'wheel' && !flattened.passive) blockingWheelListeners++;
        if (sensorListenerChanged) sensorListenerChanged(receiverFor(target), type, 1);
        if (broadcastListenerChanged) broadcastListenerChanged(receiverFor(target), type, 1);
        // A signal removes the precise listener record, including during a dispatch
        // snapshot. The abort observer is unregistered when once/manual removal wins.
        // https://dom.spec.whatwg.org/#concept-event-listener-add
        if (flattened.signal) {
            listener.abortRecord = addListener(storageFor(flattened.signal), 'abort',
                () => removeListener(target, listener), { once: true });
        }
        return listener;
    };

    const reportListenerException = error => {
        reportGlobalException(error, 'event listener');
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
        if (!(isNode(ancestor)) || !(isNode(node))) return false;
        for (let current = node; current; current = current.parentNode ||
            (current instanceof ShadowRoot ? current.host : null)) {
            if (current === ancestor) return true;
        }
        return false;
    };
    const retarget = (target, against) => {
        let adjusted = target;
        while (isNode(adjusted)) {
            const root = adjusted.getRootNode();
            if (!(root instanceof ShadowRoot) ||
                (isNode(against) && shadowIncludingContains(root, against))) return adjusted;
            adjusted = root.host;
        }
        return adjusted;
    };
    const closedShadowHidden = (node, currentTarget) => {
        if (!(isNode(node))) return false;
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
        if (!(isNode(target))) return null;
        if (target.assignedSlot) return target.assignedSlot;
        const parent = target.parentNode;
        if (parent) return parent;
        if (target instanceof ShadowRoot) {
            const targetRoot = isNode(event.__originalTarget)
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
            const targetOverride = legacyEventTargets.get(event) || target;
            if (event instanceof MouseEvent) mouseEventDispatchPositions.set(event,
                [viewportScrollX + event.clientX, viewportScrollY + event.clientY]);
            event.__dispatching = true;
            event.__originalTarget = target;
            event.__target = targetOverride;
            const hasRelatedTarget = 'relatedTarget' in event;
            const relatedTarget = hasRelatedTarget ? event.relatedTarget : null;
            let activation = null;
            const adjust = current => {
                event.__target = retarget(targetOverride, current);
                if (hasRelatedTarget) event.relatedTarget = retarget(relatedTarget, current);
                return !hasRelatedTarget || event.__target !== event.relatedTarget;
            };
            try {
                activation = prepareCheckableActivation(target, event);
                const path = eventPath(target, event);
                event.__path = [...path];
                for (let index = path.length - 1; index > 0; index--) {
                    if (!adjust(path[index])) continue;
                    invokeListeners(path[index], event, Event.CAPTURING_PHASE, true);
                }
                // The target participates in both listener passes even when the event does not
                // bubble; both passes expose AT_TARGET.
                if (adjust(target)) {
                    invokeListeners(target, event, Event.AT_TARGET, true);
                    invokeListeners(target, event, Event.AT_TARGET, false);
                }
                if (event.bubbles) {
                    for (let index = 1; index < path.length; index++) {
                        if (!adjust(path[index])) continue;
                        invokeListeners(path[index], event, Event.BUBBLING_PHASE, false);
                    }
                }
            } finally {
                if (event instanceof MouseEvent) mouseEventDispatchPositions.delete(event);
                if (hasRelatedTarget) event.relatedTarget = relatedTarget;
                event.__target = targetOverride;
                legacyEventTargets.delete(event);
                event.__phase = Event.NONE;
                event.__currentTarget = null;
                event.__path = [];
                event.__dispatching = false;
                event.__stopped = false;
                event.__immediate = false;
                event.__passive = false;
            }
            finishCheckableActivation(activation, event);
            activateLabel(target, event);
            activateFileInput(target, event);
            activateNavigation(target, event);
            activatePopoverTarget(target, event);
            popoverPointerEvent(target, event);
            return !event.defaultPrevented;
        }
    }
