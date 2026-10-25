    // HTMLObjectElement is declared with the form-associated elements; its nested
    // document accessors live with the other navigable containers.
    // https://html.spec.whatwg.org/multipage/iframe-embed-object.html#htmlobjectelement
    Object.defineProperty(HTMLObjectElement.prototype, 'getSVGDocument', {
        configurable: true, enumerable: true, writable: true,
        value: function getSVGDocument() { return contentSVGDocument(this); }
    });
