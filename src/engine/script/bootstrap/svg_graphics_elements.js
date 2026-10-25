    // SVG 2 graphics, shape and text content interfaces.
    // https://svgwg.org/svg2-draft/types.html#InterfaceSVGGraphicsElement
    class SVGGraphicsElement extends SVGElement {}
    class SVGGeometryElement extends SVGGraphicsElement {}
    // https://svgwg.org/svg2-draft/shapes.html#InterfaceSVGRectElement
    class SVGRectElement extends SVGGeometryElement {}
    for (const [name, axis] of [['x', 'x'], ['y', 'y'], ['width', 'x'], ['height', 'y']])
        filterLength(SVGRectElement.prototype, name, '0', axis);
    // https://svgwg.org/svg2-draft/text.html#InterfaceSVGTextContentElement
    class SVGTextContentElement extends SVGGraphicsElement {
        // Addressable characters are counted in UTF-16 code units. This counts the
        // descendant text without white-space collapsing or display:none exclusion.
        // https://svgwg.org/svg2-draft/text.html#__svg__SVGTextContentElement__getNumberOfChars
        getNumberOfChars() { return this.textContent.length; }
    }
    class SVGTextPositioningElement extends SVGTextContentElement {}
    class SVGTextElement extends SVGTextPositioningElement {}
    class SVGTSpanElement extends SVGTextPositioningElement {}
    // Wrapper interfaces of non-root SVG elements, keyed by local name.
    const svgElementInterfaces = Object.freeze({
        ...svgFilterElementInterfaces,
        rect: SVGRectElement,
        text: SVGTextElement,
        tspan: SVGTSpanElement
    });
    Object.assign(globalThis, {
        SVGGraphicsElement, SVGGeometryElement, SVGRectElement, SVGTextContentElement,
        SVGTextPositioningElement, SVGTextElement, SVGTSpanElement
    });
