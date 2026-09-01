    // Autonomous Custom Elements keep host DOM identity stable: upgrades replace only the
    // wrapper prototype, and the construction stack makes HTMLElement's super() return it.
    const customElementStates = new WeakMap();
    const customElementDefinitions = new WeakMap();
    const registryStates = new WeakMap();
    const definitionsByConstructor = new Map();
    const alreadyConstructedMarker = {};
    const reservedCustomElementNames = new Set([
        'annotation-xml', 'color-profile', 'font-face', 'font-face-src', 'font-face-uri',
        'font-face-format', 'font-face-name', 'missing-glyph'
    ]);
    // HTML defines this in terms of a valid element local name plus an ASCII-lowercase
    // first character, a hyphen, and no ASCII uppercase. Keep the grammar explicit so
    // punctuation rejected by the tokenizer cannot become a registry key.
    const validCustomElementName = name =>
        /^[a-z][a-z0-9._:\-\u0080-\u{10ffff}]*$/u.test(name) &&
        name.includes('-') && !reservedCustomElementNames.has(name);
    const isConstructor = value => {
        if (typeof value !== 'function') return false;
        try { Reflect.construct(String, [], value); return true; }
        catch (_) { return false; }
    };

    // https://html.spec.whatwg.org/multipage/custom-elements.html#custom-element-reactions-stack
    // Every [CEReactions] operation gets its own element queue. In particular, an attribute
    // mutation made by a lifecycle callback must drain its nested queue before returning to that
    // callback; flattening all reactions into one queue breaks reflection guards used by Polymer.
    const elementReactionQueues = new WeakMap();
    const reactionStack = [];
    const invokeCustomElementReactions = elementQueue => {
        for (let elementIndex = 0; elementIndex < elementQueue.length; elementIndex++) {
            const element = elementQueue[elementIndex];
            const reactions = elementReactionQueues.get(element);
            if (!reactions?.length) continue;
            elementReactionQueues.set(element, []);
            for (let reactionIndex = 0; reactionIndex < reactions.length; reactionIndex++) {
                const reaction = reactions[reactionIndex];
                try { reaction.callback.apply(element, reaction.args); }
                catch (error) { reportGlobalException(error); }
            }
        }
    };
    const withCustomElementReactions = callback => {
        const elementQueue = [];
        reactionStack.push(elementQueue);
        try { return callback(); }
        finally {
            reactionStack.pop();
            invokeCustomElementReactions(elementQueue);
        }
    };
    const enqueueCustomElementCallback = (element, callbackName, args = []) => {
        if (customElementStates.get(element) !== 'custom') return;
        const definition = customElementDefinitions.get(element);
        const callback = definition?.callbacks[callbackName];
        if (!callback) return;
        if (callbackName === 'attributeChangedCallback' && !definition.observedAttributes.includes(args[0])) return;
        let reactions = elementReactionQueues.get(element);
        if (!reactions) elementReactionQueues.set(element, reactions = []);
        reactions.push({ callback, args });
        const elementQueue = reactionStack[reactionStack.length - 1];
        if (elementQueue) elementQueue.push(element);
        else invokeCustomElementReactions([element]);
    };
    const inclusiveElementDescendants = root => {
        if (!(root instanceof Node)) return [];
        const elements = [];
        const pending = [root];
        while (pending.length) {
            const node = pending.pop();
            if (node instanceof Element) elements.push(node);
            const children = node.childNodes;
            for (let index = children.length - 1; index >= 0; index--) pending.push(children[index]);
            const shadowRoot = shadowRootForTraversal(node);
            if (shadowRoot) pending.push(shadowRoot);
        }
        return elements;
    };

    let defaultCustomElementRegistry = null;
    const definitionForElement = (registry, element) => {
        if (!registry || element.namespaceURI !== htmlNamespace) return null;
        return registryStates.get(registry)?.definitionsByName.get(element.localName) || null;
    };
    const upgradeElement = (element, definition, synchronous) => {
        const state = customElementStates.get(element);
        if (state === 'custom' || state === 'failed') return element;
        const initialAttributes = attributeRecords(element).filter(record =>
            definition.observedAttributes.includes(record.localName));
        customElementDefinitions.set(element, definition);
        customElementStates.set(element, 'failed');
        Object.setPrototypeOf(element, definition.prototype);
        definition.constructionStack.push(element);
        try {
            const constructed = new definition.constructor();
            if (constructed !== element)
                throw new TypeError('Custom element constructors must return the element being upgraded');
            customElementStates.set(element, 'custom');
        } catch (error) {
            if (synchronous) throw error;
            reportGlobalException(error);
            return element;
        } finally {
            definition.constructionStack.pop();
        }
        withCustomElementReactions(() => {
            for (const record of initialAttributes)
                enqueueCustomElementCallback(element, 'attributeChangedCallback',
                    [record.localName, null, record.value, record.namespace]);
            if (element.isConnected) enqueueCustomElementCallback(element, 'connectedCallback');
        });
        return element;
    };
    const tryUpgradeElement = (element, registry, synchronous = false) => {
        if (!(element instanceof Element)) return element;
        const definition = definitionForElement(registry, element);
        return definition ? upgradeElement(element, definition, synchronous) : element;
    };

    maybeUpgradeCustomElement = (element, synchronous = false) =>
        tryUpgradeElement(element, defaultCustomElementRegistry, synchronous);
    upgradeCustomElementTree = (root, registry = defaultCustomElementRegistry) =>
        withCustomElementReactions(() => {
            for (const element of inclusiveElementDescendants(root)) tryUpgradeElement(element, registry);
        });
    connectCustomElementTree = root => withCustomElementReactions(() => {
        for (const element of inclusiveElementDescendants(root)) {
            const wasCustom = customElementStates.get(element) === 'custom';
            tryUpgradeElement(element, defaultCustomElementRegistry);
            if (wasCustom) enqueueCustomElementCallback(element, 'connectedCallback');
        }
    });
    disconnectCustomElementTree = root => withCustomElementReactions(() => {
        for (const element of inclusiveElementDescendants(root))
            enqueueCustomElementCallback(element, 'disconnectedCallback');
    });
    adoptCustomElementTree = (root, oldDocument, newDocument) => withCustomElementReactions(() => {
        resetAttributeNameMode(root);
        for (const element of inclusiveElementDescendants(root))
            enqueueCustomElementCallback(element, 'adoptedCallback', [oldDocument, newDocument]);
    });
    customElementAttributeChanged = (element, name, oldValue, newValue, namespace) =>
        withCustomElementReactions(() => enqueueCustomElementCallback(element,
            'attributeChangedCallback', [name, oldValue, newValue, namespace]));

    constructCustomElement = constructor => {
        const definition = definitionsByConstructor.get(constructor);
        if (!definition) throw new TypeError('Invalid custom element constructor');
        const stack = definition.constructionStack;
        if (stack.length) {
            const index = stack.length - 1;
            const element = stack[index];
            if (element === alreadyConstructedMarker)
                throw new DOMException('The custom element was already constructed', 'InvalidStateError');
            stack[index] = alreadyConstructedMarker;
            return element;
        }
        const element = wrap(host('createElement', document.__id, definition.localName));
        Object.setPrototypeOf(element, definition.prototype);
        customElementDefinitions.set(element, definition);
        customElementStates.set(element, 'custom');
        return element;
    };

    class CustomElementRegistry {
        constructor() {
            registryStates.set(this, {
                definitionsByName: new Map(),
                definitionsByConstructor: new Map(),
                whenDefined: new Map(),
                defining: false,
                scoped: true
            });
        }
        define(name, constructor, options = {}) {
            name = String(name);
            const state = registryStates.get(this);
            if (!isConstructor(constructor)) throw new TypeError('Custom element constructor is not constructible');
            if (!validCustomElementName(name))
                throw new DOMException('The custom element name is invalid', 'SyntaxError');
            if (state.definitionsByName.has(name) || state.definitionsByConstructor.has(constructor))
                throw new DOMException('The custom element is already defined', 'NotSupportedError');
            options = Object(options);
            if (options.extends !== undefined)
                throw new DOMException('Customized built-in elements are not supported', 'NotSupportedError');
            if (state.defining)
                throw new DOMException('A custom element definition is already running', 'NotSupportedError');
            state.defining = true;
            let definition;
            try {
                const prototype = constructor.prototype;
                if ((typeof prototype !== 'object' && typeof prototype !== 'function') || prototype === null)
                    throw new TypeError('Custom element constructor prototype must be an object');
                const callbacks = {};
                for (const name of ['connectedCallback', 'disconnectedCallback', 'adoptedCallback',
                    'attributeChangedCallback']) {
                    const callback = prototype[name];
                    if (callback !== undefined && typeof callback !== 'function')
                        throw new TypeError(name + ' must be a function');
                    callbacks[name] = callback || null;
                }
                const observedAttributes = callbacks.attributeChangedCallback && constructor.observedAttributes !== undefined
                    ? Array.from(constructor.observedAttributes, String) : [];
                definition = {
                    name,
                    localName: name,
                    constructor,
                    prototype,
                    callbacks,
                    observedAttributes,
                    constructionStack: [],
                    registry: this
                };
            } finally {
                state.defining = false;
            }
            state.definitionsByName.set(name, definition);
            state.definitionsByConstructor.set(constructor, definition);
            definitionsByConstructor.set(constructor, definition);
            if (this === defaultCustomElementRegistry) upgradeCustomElementTree(document, this);
            const pending = state.whenDefined.get(name);
            if (pending) {
                state.whenDefined.delete(name);
                pending.resolve(constructor);
            }
        }
        get(name) { return registryStates.get(this).definitionsByName.get(String(name))?.constructor; }
        getName(constructor) {
            if (!isConstructor(constructor)) throw new TypeError('Custom element interface is not constructible');
            return registryStates.get(this).definitionsByConstructor.get(constructor)?.name ?? null;
        }
        whenDefined(name) {
            name = String(name);
            if (!validCustomElementName(name))
                return Promise.reject(new DOMException('The custom element name is invalid', 'SyntaxError'));
            const state = registryStates.get(this);
            const definition = state.definitionsByName.get(name);
            if (definition) return Promise.resolve(definition.constructor);
            let pending = state.whenDefined.get(name);
            if (!pending) {
                pending = {};
                pending.promise = new Promise(resolve => { pending.resolve = resolve; });
                state.whenDefined.set(name, pending);
            }
            return pending.promise;
        }
        upgrade(root) {
            if (!(root instanceof Node)) throw new TypeError('CustomElementRegistry.upgrade requires a Node');
            const state = registryStates.get(this);
            if (!state.scoped && (root instanceof Document || root.ownerDocument !== document))
                throw new DOMException('The root does not use this custom element registry', 'NotSupportedError');
            upgradeCustomElementTree(root, this);
        }
    }
    Object.defineProperty(CustomElementRegistry.prototype, Symbol.toStringTag,
        { value: 'CustomElementRegistry', configurable: true });
    defaultCustomElementRegistry = new CustomElementRegistry();
    registryStates.get(defaultCustomElementRegistry).scoped = false;
    windowObject.CustomElementRegistry = CustomElementRegistry;
    Object.defineProperty(windowObject, 'customElements', {
        value: defaultCustomElementRegistry,
        writable: false,
        enumerable: true,
        configurable: true
    });
