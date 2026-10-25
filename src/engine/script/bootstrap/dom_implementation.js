    // DOMImplementation creates documents and doctypes owned by its associated document.
    // https://dom.spec.whatwg.org/#valid-element-local-name
    const validElementLocalName = name => /^[A-Za-z][^\0\t\n\f\r \/>]*$/u.test(name) ||
        /^[:_\u0080-\u{10FFFF}][-.:_A-Za-z0-9\u0080-\u{10FFFF}]*$/u.test(name);
    // https://dom.spec.whatwg.org/#validate-and-extract (element context)
    const validateElementQualifiedName = (namespace, qualifiedName) => {
        const separator = qualifiedName.indexOf(':');
        const prefix = separator < 0 ? null : qualifiedName.slice(0, separator);
        const localName = separator < 0 ? qualifiedName : qualifiedName.slice(separator + 1);
        if ((prefix !== null && (!prefix || /[\0\t\n\f\r \/>]/u.test(prefix))) ||
            !validElementLocalName(localName))
            throw new DOMException('The element name is invalid', 'InvalidCharacterError');
        const namespaceError = message => { throw new DOMException(message, 'NamespaceError'); };
        if (prefix !== null && !namespace) namespaceError('A prefix requires a namespace');
        if (prefix === 'xml' && namespace !== 'http://www.w3.org/XML/1998/namespace')
            namespaceError('The xml prefix requires the XML namespace');
        const xmlns = 'http://www.w3.org/2000/xmlns/';
        if ((qualifiedName === 'xmlns' || prefix === 'xmlns') !== (namespace === xmlns))
            namespaceError('The xmlns name and the XMLNS namespace must be used together');
    };
    const implementationDocuments = new WeakMap();
    const documentImplementations = new WeakMap();
    class DOMImplementation {
        createDocumentType(name, publicId, systemId) {
            name = String(name);
            // https://dom.spec.whatwg.org/#valid-doctype-name
            if (/[\0\t\n\f\r >]/u.test(name))
                throw new DOMException('The doctype name is invalid', 'InvalidCharacterError');
            const owner = implementationDocuments.get(this);
            if (!owner) throw new TypeError('Illegal invocation');
            return wrap(host('createDocumentType', nodeId(owner), name, String(publicId), String(systemId)));
        }
        createDocument(namespace, qualifiedName, doctype = null) {
            if (doctype !== null && !(doctype instanceof DocumentType))
                throw new TypeError('createDocument() doctype must be a DocumentType or null');
            // qualifiedName is [LegacyNullToEmptyString]: null creates no document element.
            // https://dom.spec.whatwg.org/#dom-domimplementation-createdocument
            const name = qualifiedName === null ? '' : String(qualifiedName);
            if (name) validateElementQualifiedName(namespace == null ? '' : String(namespace), name);
            const result = wrap(host('createDocument', namespace == null ? '' : String(namespace), name));
            Object.setPrototypeOf(result, XMLDocument.prototype);
            // The doctype is appended before the element, adopting it into the new document.
            if (doctype !== null) result.insertBefore(doctype, result.firstChild);
            return result;
        }
        createHTMLDocument(title = '') { return wrap(host('createHtmlDocument', String(title))); }
    }

    const implementationFor = document => {
        let implementation = documentImplementations.get(document);
        if (!implementation) {
            implementation = new DOMImplementation();
            documentImplementations.set(document, implementation);
            implementationDocuments.set(implementation, document);
        }
        return implementation;
    };
