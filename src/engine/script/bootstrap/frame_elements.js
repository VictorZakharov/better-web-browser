    // Navigable lifetimes belong to DOM connection/removal, independently of custom elements.
    // https://html.spec.whatwg.org/multipage/iframe-embed-object.html#the-iframe-element
    const connectFrameTree = root => {
        for (const element of inclusiveElementDescendants(root))
            if (element.localName === 'iframe') host('frameWindow', element.__id, element);
    };
    const disconnectFrameTree = root => {
        for (const element of inclusiveElementDescendants(root))
            if (element.localName === 'iframe') host('discardFrame', element.__id);
    };
    const connectElementTree = root => {
        connectFrameTree(root);
        connectCustomElementTree(root);
    };
    const disconnectElementTree = root => {
        disconnectFrameTree(root);
        disconnectCustomElementTree(root);
    };
