    // HTML "create an element for a token": constructor, all attributes, reactions,
    // then insertion. The native tree builder resumes only after these steps return.
    function constructParserElement(target, element, network = false) {
        const previousCheckpoint = parserReactionCheckpoint;
        parserReactionCheckpoint = network;
        try { constructParserElementInContext(target, element, network); }
        finally { parserReactionCheckpoint = previousCheckpoint; }
    }
    function constructParserElementInContext(target, element, network) {
        const intendedParent = wrap(host('parserElementIntendedParent', nodeId(target)));
        const registry = intendedParent ? nodeRegistryFor(intendedParent) : documentRegistryFor(target);
        rememberElementRegistry(element, registry);
        const definition = definitionForElement(element);
        // A parser-created custom element keeps the dynamic-markup insertion
        // guard through its initial attribute reactions. Connected reactions
        // run after insertion and may write at the parser's insertion point.
        if (definition) {
            throwOnDynamicMarkupInsertion++;
        }
        try {
            if (definition) {
                if (network) host('parserMicrotaskCheckpoint');
                try {
                    const localName = element.localName;
                    customElementCallbackDepth++;
                    const previousConstructionRegistry = activeConstructionRegistries.get(definition.constructor);
                    activeConstructionRegistries.set(definition.constructor, definition.registry);
                    let constructed;
                    try { constructed = new definition.constructor(); }
                    finally {
                        if (previousConstructionRegistry === undefined)
                            activeConstructionRegistries.delete(definition.constructor);
                        else activeConstructionRegistries.set(definition.constructor, previousConstructionRegistry);
                        customElementCallbackDepth--;
                    }
                    if (network) host('parserMicrotaskCheckpoint');
                    if (!(constructed instanceof HTMLElement))
                        throw new TypeError('Custom element constructors must return an HTMLElement');
                    if (constructed.attributes.length || constructed.childNodes.length || constructed.parentNode ||
                        constructed.ownerDocument !== target || constructed.namespaceURI !== htmlNamespace ||
                        constructed.localName !== localName)
                        throw new DOMException('A parser custom element constructor must return an empty, detached element',
                            'NotSupportedError');
                    host('parserElementResult', nodeId(target), nodeId(constructed));
                    element = constructed;
                } catch (error) {
                    if (network) host('parserMicrotaskCheckpoint');
                    reportGlobalException(error);
                    const failed = host('parserElementFailed', nodeId(target));
                    element = wrap(failed.node);
                    Object.setPrototypeOf(element, HTMLUnknownElement.prototype);
                    rememberElementRegistry(element, registry);
                    customElementStates.set(element, 'failed');
                }
            }
            const attributes = host('parserElementAttributes', nodeId(target));
            parserDomChanged(attributes.ids, attributes.mutations);
            withCustomElementReactions(() => {
                for (const attribute of attributeRecords(element)) {
                    eventHandlerAttributeChanged(element, attribute, attribute.value);
                    enqueueCustomElementCallback(element, 'attributeChangedCallback',
                        [attribute.localName, null, attribute.value, attribute.namespace]);
                }
            });
        } finally {
            if (definition) throwOnDynamicMarkupInsertion--;
        }
        const inserted = host('parserElementInsert', nodeId(target));
        parserDomChanged(inserted.ids, inserted.mutations);
        if (element.isConnected) withCustomElementReactions(() =>
            enqueueCustomElementCallback(element, 'connectedCallback'));
    }
    globalThis.__constructParserElement = (target, node) => constructParserElement(wrap(target), wrap(node), true);
