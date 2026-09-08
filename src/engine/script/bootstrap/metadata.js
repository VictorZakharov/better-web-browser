    // HTML metadata uses child text, not descendant text or rendered headings.
    // https://html.spec.whatwg.org/multipage/semantics.html#the-title-element
    const htmlTitleConstructionToken = {};
    const htmlTitleElements = new WeakSet();
    const metadataNodeText = Object.getOwnPropertyDescriptor(Node.prototype, 'textContent');
    const metadataChildNodes = Object.getOwnPropertyDescriptor(Node.prototype, 'childNodes').get;
    const metadataString = value => {
        if (typeof value === 'symbol') throw new TypeError('Cannot convert a Symbol to DOMString');
        return String(value);
    };
    const metadataChildText = node => {
        if (!node) return '';
        let text = '';
        for (const child of metadataChildNodes.call(node)) {
            if (child.nodeType === Node.TEXT_NODE || child.nodeType === Node.CDATA_SECTION_NODE)
                text += metadataNodeText.get.call(child);
        }
        return text;
    };
    class HTMLTitleElement extends HTMLElement {
        constructor(id, type, name, localName, namespace, token) {
            if (token !== htmlTitleConstructionToken) throw new TypeError('Illegal constructor');
            super(id, type, name, localName, namespace);
            htmlTitleElements.add(this);
        }
        get text() {
            if (!htmlTitleElements.has(this)) throw new TypeError('Illegal invocation');
            return metadataChildText(this);
        }
        set text(value) {
            if (!htmlTitleElements.has(this)) throw new TypeError('Illegal invocation');
            const text = metadataString(value);
            withCustomElementReactions(() => metadataNodeText.set.call(this, text));
        }
    }
    Object.defineProperty(HTMLTitleElement.prototype, Symbol.toStringTag,
        { configurable: true, value: 'HTMLTitleElement' });
    Object.defineProperty(HTMLTitleElement.prototype, 'text', {
        ...Object.getOwnPropertyDescriptor(HTMLTitleElement.prototype, 'text'), enumerable: true
    });

    const metadataElementIs = (node, namespace, name) =>
        node?.nodeType === Node.ELEMENT_NODE && node.namespaceURI === namespace && node.localName === name;
    const metadataHtmlTitle = document => [...document.querySelectorAll('title')]
        .find(node => metadataElementIs(node, htmlNamespace, 'title')) || null;
    const metadataSvgTitle = root => [...root.children]
        .find(node => metadataElementIs(node, svgNamespace, 'title')) || null;
    // https://html.spec.whatwg.org/multipage/dom.html#document.title
    const documentTitleValue = document => {
        const root = document.documentElement;
        const title = metadataElementIs(root, svgNamespace, 'svg')
            ? metadataSvgTitle(root) : metadataHtmlTitle(document);
        return metadataChildText(title).replace(/[\t\n\f\r ]+/g, ' ').replace(/^ | $/g, '');
    };
    const setDocumentTitleValue = (document, value) => {
        const text = metadataString(value);
        const root = document.documentElement;
        let title;
        if (metadataElementIs(root, svgNamespace, 'svg')) {
            title = metadataSvgTitle(root);
            if (!title) {
                title = document.createElementNS(svgNamespace, 'title');
                root.insertBefore(title, root.firstChild);
            }
        } else if (root?.namespaceURI === htmlNamespace) {
            title = metadataHtmlTitle(document);
            if (!title) {
                const head = metadataElementIs(root, htmlNamespace, 'html')
                    ? [...root.children].find(node => metadataElementIs(node, htmlNamespace, 'head')) : null;
                if (!head) return;
                title = document.createElementNS(htmlNamespace, 'title');
                head.appendChild(title);
            }
        } else return;
        metadataNodeText.set.call(title, text);
    };
