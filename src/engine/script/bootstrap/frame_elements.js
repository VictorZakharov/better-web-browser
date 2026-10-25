    // Navigable lifetimes belong to DOM connection/removal, independently of custom elements.
    // https://html.spec.whatwg.org/multipage/iframe-embed-object.html#the-iframe-element
    // iframe and object elements are navigable containers; the native side gives an
    // object a content navigable only for its data resource.
    // https://html.spec.whatwg.org/multipage/document-sequences.html#navigable-container
    const navigableContainer = element =>
        element.localName === 'iframe' || element.localName === 'object';
    // HTML re-runs an object's resource selection when its data or type changes; a newly
    // acceptable resource needs a content navigable, an unacceptable one discards it.
    // https://html.spec.whatwg.org/multipage/iframe-embed-object.html#the-object-element
    const objectResourceAttributeChanged = element => {
        if (element.isConnected) host('objectResourceChanged', nodeId(element), element);
    };
    const connectFrameTree = root => {
        for (const element of inclusiveElementDescendants(root))
            if (navigableContainer(element)) host('frameWindow', nodeId(element), element);
    };
    const disconnectFrameTree = root => {
        for (const element of inclusiveElementDescendants(root))
            if (navigableContainer(element)) host('discardFrame', nodeId(element));
    };
    const connectElementTree = root => {
        connectFrameTree(root);
        connectCustomElementTree(root);
    };
    const disconnectElementTree = root => {
        disconnectPopoverTree(root);
        disconnectFrameTree(root);
        disconnectCustomElementTree(root);
    };
