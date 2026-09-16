    // HTML "create an element for a token": constructor, all attributes, reactions,
    // then insertion. The native tree builder resumes only after these steps return.
    function constructParserElement(target, element, network = false) {
        const previousCheckpoint = parserReactionCheckpoint;
        parserReactionCheckpoint = network;
        try { constructParserElementInContext(target, element, network); }
        finally { parserReactionCheckpoint = previousCheckpoint; }
    }
    function constructParserElementInContext(target, element, network) {
        throwOnDynamicMarkupInsertion++;
        try {
            if (network) host('parserMicrotaskCheckpoint');
            try {
                const definition = definitionForElement(defaultCustomElementRegistry, element);
                const localName = element.localName;
                customElementCallbackDepth++;
                let constructed;
                try { constructed = new definition.constructor(); }
                finally { customElementCallbackDepth--; }
                if (network) host('parserMicrotaskCheckpoint');
                if (!(constructed instanceof HTMLElement))
                    throw new TypeError('Custom element constructors must return an HTMLElement');
                if (constructed.attributes.length || constructed.childNodes.length || constructed.parentNode ||
                    constructed.ownerDocument !== target || constructed.namespaceURI !== htmlNamespace ||
                    constructed.localName !== localName)
                    throw new DOMException('A parser custom element constructor must return an empty, detached element',
                        'NotSupportedError');
                host('parserElementResult', target.__id, constructed.__id);
                element = constructed;
            } catch (error) {
                if (network) host('parserMicrotaskCheckpoint');
                reportGlobalException(error);
                const failed = host('parserElementFailed', target.__id);
                element = wrap(failed.node);
                Object.setPrototypeOf(element, HTMLUnknownElement.prototype);
                customElementStates.set(element, 'failed');
            }
            const attributes = host('parserElementAttributes', target.__id);
            parserDomChanged(attributes.ids, attributes.mutations);
            withCustomElementReactions(() => {
                for (const attribute of attributeRecords(element)) {
                    eventHandlerAttributeChanged(element, attribute, attribute.value);
                    enqueueCustomElementCallback(element, 'attributeChangedCallback',
                        [attribute.localName, null, attribute.value, attribute.namespace]);
                }
            });
        } finally {
            throwOnDynamicMarkupInsertion--;
        }
        const inserted = host('parserElementInsert', target.__id);
        parserDomChanged(inserted.ids, inserted.mutations);
        if (element.isConnected) withCustomElementReactions(() =>
            enqueueCustomElementCallback(element, 'connectedCallback'));
    }
    globalThis.__constructParserElement = (target, node) => constructParserElement(wrap(target), wrap(node), true);
