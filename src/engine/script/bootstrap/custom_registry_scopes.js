    // DOM/HTML assign a registry to each Element, Document, and ShadowRoot.
    // Wrapper identity is stable for this realm, so native nodes keep their
    // registry identity here while native shadow metadata records the global
    // versus non-global distinction needed by serialization.
    const elementRegistries = new WeakMap();
    const shadowRegistries = new WeakMap();
    const documentRegistries = new WeakMap();
    const documentRegistryFor = owner => owner === document
        ? defaultCustomElementRegistry : documentRegistries.get(owner) || null;
    const rememberElementRegistry = (element, registry) => {
        elementRegistries.set(element, registry);
        if (registry && registryStates.get(registry)?.scoped)
            registryStates.get(registry).documents.add(wrap(host('ownerDocument', nodeId(element))));
        return registry;
    };
    const rememberShadowRegistry = (root, registry) => {
        shadowRegistries.set(root, registry);
        if (registry && registryStates.get(registry)?.scoped)
            registryStates.get(registry).documents.add(wrap(host('ownerDocument', nodeId(root))));
        return registry;
    };
    const shadowRegistryFor = root => {
        if (shadowRegistries.has(root)) return shadowRegistries.get(root);
        if (host('shadowRegistryIsNull', nodeId(root))) return rememberShadowRegistry(root, null);
        if (host('shadowRegistryIsGlobal', nodeId(root)))
            return rememberShadowRegistry(root,
                documentRegistryFor(wrap(host('ownerDocument', nodeId(root)))));
        throw new DOMException('Scoped shadow registry identity is unavailable', 'InvalidStateError');
    };
    const elementRegistryFor = element => {
        if (elementRegistries.has(element)) return elementRegistries.get(element);
        const parent = wrap(host('parent', nodeId(element)));
        const parentId = nodeId(parent);
        const parentType = nativeNodeType(parent);
        const registry = parentType === 11 && host('shadowHost', parentId)
            ? shadowRegistryFor(parent)
            : parentType === 1 ? elementRegistryFor(parent)
                : parentType === 9 ? documentRegistryFor(parent)
                    : documentRegistryFor(wrap(host('ownerDocument', nodeId(element))));
        return rememberElementRegistry(element, registry);
    };
    const nodeRegistryFor = node => {
        const id = nodeId(node);
        const type = nativeNodeType(node);
        return type === 1 ? elementRegistryFor(node)
            : type === 11 && host('shadowHost', id) ? shadowRegistryFor(node)
                : type === 9 ? documentRegistryFor(node) : null;
    };
    const checkedRegistry = (value, owner) => {
        if (value == null) return null;
        if (!registryStates.has(value)) throw new TypeError('Expected a CustomElementRegistry');
        if (!registryStates.get(value).scoped && value !== documentRegistryFor(owner))
            throw new DOMException('A global registry belongs to another document', 'NotSupportedError');
        return value;
    };
    const registryForCreationOptions = (owner, options) => {
        let registry = documentRegistryFor(owner);
        if (options == null) return registry;
        if (typeof options === 'string')
            throw new DOMException('Customized built-in elements are not supported', 'NotSupportedError');
        options = Object(options);
        if ('is' in options)
            throw new DOMException('Customized built-in elements are not supported', 'NotSupportedError');
        if ('customElementRegistry' in options)
            registry = checkedRegistry(options.customElementRegistry, owner);
        return registry;
    };
    const registryForShadowInit = (owner, init) => 'customElementRegistry' in init
        ? checkedRegistry(init.customElementRegistry, owner) : documentRegistryFor(owner);
    const importRegistryOptions = (owner, options) => {
        if (typeof options !== 'object' || options === null)
            return { deep: !!options, fallback: documentRegistryFor(owner) };
        const deep = !options.selfOnly;
        let fallback = documentRegistryFor(owner);
        if ('customElementRegistry' in options) {
            const requested = options.customElementRegistry;
            // ImportNodeOptions uses a non-nullable registry member, unlike
            // ElementCreationOptions and ShadowRootInit.
            if (!registryStates.has(requested)) throw new TypeError('Expected a CustomElementRegistry');
            fallback = checkedRegistry(requested, owner);
        }
        return { deep, fallback };
    };
    const ordinaryElementDescendants = root => {
        const elements = [];
        const pending = [root];
        while (pending.length) {
            const node = pending.pop();
            if (node instanceof Element) elements.push(node);
            const children = node.childNodes;
            for (let index = children.length - 1; index >= 0; index--) pending.push(children[index]);
        }
        return elements;
    };
    // A fallback registry applies to ordinary cloned descendants, but not to
    // descendants inside a cloned shadow root (DOM clone-a-node step 6.7).
    const transferClonedRegistries = (source, copy, targetDocument, fallback = null) => {
        const pending = [[source, copy, fallback]];
        while (pending.length) {
            const [original, cloned, inheritedFallback] = pending.pop();
            if (original instanceof Element) {
                let registry = elementRegistryFor(original) || inheritedFallback;
                if (registry && !registryStates.get(registry).scoped)
                    registry = documentRegistryFor(targetDocument);
                rememberElementRegistry(cloned, registry);
                const originalShadow = shadowRootForTraversal(original);
                const clonedShadow = shadowRootForTraversal(cloned);
                if (originalShadow && clonedShadow) {
                    let shadowRegistry = shadowRegistryFor(originalShadow);
                    if (shadowRegistry && !registryStates.get(shadowRegistry).scoped)
                        shadowRegistry = documentRegistryFor(targetDocument);
                    rememberShadowRegistry(clonedShadow, shadowRegistry);
                    pending.push([originalShadow, clonedShadow, null]);
                }
                if (original.localName === 'template' && cloned.localName === 'template')
                    pending.push([original.content, cloned.content, null]);
            }
            const from = original.childNodes;
            const to = cloned.childNodes;
            for (let index = from.length - 1; index >= 0 && index < to.length; index--)
                pending.push([from[index], to[index], inheritedFallback]);
        }
    };
    const captureRegistryTree = root => {
        for (const element of inclusiveElementDescendants(root)) {
            elementRegistryFor(element);
            const shadow = shadowRootForTraversal(element);
            if (shadow) shadowRegistryFor(shadow);
        }
    };
    const adoptRegistries = (root, newDocument) => {
        const targetRegistry = documentRegistryFor(newDocument);
        const targetGlobal = targetRegistry && !registryStates.get(targetRegistry).scoped
            ? targetRegistry : null;
        for (const element of inclusiveElementDescendants(root)) {
            const registry = elementRegistryFor(element);
            if (registry === null || !registryStates.get(registry).scoped) {
                let nextRegistry = targetGlobal;
                if (registry === null) {
                    const parent = element.parentNode;
                    const parentRegistry = parent instanceof Element || parent instanceof ShadowRoot ||
                        parent instanceof Document ? nodeRegistryFor(parent) : targetGlobal;
                    nextRegistry = parentRegistry && !registryStates.get(parentRegistry).scoped
                        ? parentRegistry : null;
                }
                rememberElementRegistry(element, nextRegistry);
            }
            const shadow = shadowRootForTraversal(element);
            if (shadow) {
                const shadowRegistry = shadowRegistryFor(shadow);
                const keepNull = !!host('shadowKeepRegistryNull', nodeId(shadow));
                if ((shadowRegistry === null && !keepNull) ||
                    (shadowRegistry && !registryStates.get(shadowRegistry).scoped)) {
                    if (!host('shadowRegistryAdopt', nodeId(shadow), targetGlobal !== null,
                        targetGlobal === null))
                        throw new DOMException('Could not adopt the shadow registry', 'InvalidStateError');
                    rememberShadowRegistry(shadow, targetGlobal);
                }
            }
        }
    };
