    // DOM exposes one live NamedNodeMap per element and stable Attr wrappers. The Proxy
    // preserves Web IDL's indexed-before-prototype-before-named property lookup order.
    const attributeConstructionToken = {};
    const attributeCollections = new WeakMap();
    const namedNodeMapElements = new WeakMap();
    const xmlNamespace = 'http://www.w3.org/XML/1998/namespace';
    const xmlnsNamespace = 'http://www.w3.org/2000/xmlns/';

    const normalizedNamespace = namespace => {
        namespace = namespace == null ? null : String(namespace);
        return namespace === '' ? null : namespace;
    };
    const validAttributeLocalName = name => name.length > 0 && !/[\0\t\n\f\r \/=>]/u.test(name);
    const validateAttributeLocalName = name => {
        if (!validAttributeLocalName(name))
            throw new DOMException('The attribute name is invalid', 'InvalidCharacterError');
        return name;
    };
    const validateAndExtractAttributeName = (namespace, qualifiedName) => {
        namespace = normalizedNamespace(namespace);
        qualifiedName = String(qualifiedName);
        let prefix = null;
        let localName = qualifiedName;
        const separator = qualifiedName.indexOf(':');
        if (separator >= 0) {
            prefix = qualifiedName.slice(0, separator);
            localName = qualifiedName.slice(separator + 1);
            if (!prefix || /[\0\t\n\f\r \/>]/u.test(prefix))
                throw new DOMException('The namespace prefix is invalid', 'InvalidCharacterError');
        }
        validateAttributeLocalName(localName);
        if (prefix !== null && namespace === null)
            throw new DOMException('A prefix requires a namespace', 'NamespaceError');
        if (prefix === 'xml' && namespace !== xmlNamespace)
            throw new DOMException('The xml prefix requires the XML namespace', 'NamespaceError');
        if ((qualifiedName === 'xmlns' || prefix === 'xmlns') && namespace !== xmlnsNamespace)
            throw new DOMException('The xmlns name requires the XMLNS namespace', 'NamespaceError');
        if (namespace === xmlnsNamespace && qualifiedName !== 'xmlns' && prefix !== 'xmlns')
            throw new DOMException('The XMLNS namespace requires an xmlns name', 'NamespaceError');
        return { namespace, prefix, localName, qualifiedName };
    };
    const qualifiedAttributeName = (prefix, localName) => prefix === null ? localName : prefix + ':' + localName;
    const attributeKey = (namespace, localName) => (namespace || '') + '\u001f' + localName;
    const usesHtmlAttributeNames = element =>
        element.namespaceURI === htmlNamespace && host('isHtmlDocument', element.__id);
    const normalizedQualifiedName = (element, name) => {
        name = String(name);
        return usesHtmlAttributeNames(element) ? name.toLowerCase() : name;
    };
    const attributeRecords = element => JSON.parse(host('attrRecords', element.__id));
    const cacheForAttributes = element => {
        let state = attributeCollections.get(element);
        if (!state) {
            state = { attributes: new Map(), collection: null };
            attributeCollections.set(element, state);
        }
        return state;
    };

    class Attr extends Node {
        constructor(token, ownerDocument, record, ownerElement = null) {
            if (token !== attributeConstructionToken) throw new TypeError('Illegal constructor');
            super(0, 2, record.qualifiedName, record.localName, record.namespace);
            this.__document = ownerDocument;
            this.__prefix = record.prefix;
            this.__value = record.value;
            this.__element = ownerElement;
        }
        get namespaceURI() { return this.__namespaceURI; }
        get prefix() { return this.__prefix; }
        get localName() { return this.__localName; }
        get name() { return this.__nodeName; }
        get value() {
            if (!this.__element) return this.__value;
            const value = host('attrGetNs', this.__element.__id, this.namespaceURI || '', this.localName);
            return value === null ? this.__value : (this.__value = value);
        }
        set value(value) {
            value = String(value);
            if (!this.__element) { this.__value = value; return; }
            setAttachedAttributeValue(this, value);
        }
        get nodeValue() { return this.value; }
        set nodeValue(value) { this.value = value == null ? '' : String(value); }
        get textContent() { return this.value; }
        set textContent(value) { this.value = value == null ? '' : String(value); }
        get ownerElement() { return this.__element; }
        get ownerDocument() { return this.__document; }
        get specified() { return true; }
        get parentNode() { return null; }
        get parentElement() { return null; }
        get firstChild() { return null; }
        get lastChild() { return null; }
        get nextSibling() { return null; }
        get previousSibling() { return null; }
        get childNodes() { const nodes = []; nodes.item = () => null; return nodes; }
        get children() { const nodes = []; nodes.item = () => null; return nodes; }
        get isConnected() { return false; }
        hasChildNodes() { return false; }
        contains(other) { return other === this; }
        querySelector() { return null; }
        querySelectorAll() { const nodes = []; nodes.item = () => null; return nodes; }
        appendChild() { throw new DOMException('Attributes cannot have children', 'HierarchyRequestError'); }
        insertBefore() { throw new DOMException('Attributes cannot have children', 'HierarchyRequestError'); }
        removeChild() { throw new DOMException('Attributes do not have children', 'NotFoundError'); }
        cloneNode() { return createDetachedAttribute(this.ownerDocument, this.namespaceURI, this.prefix, this.localName, this.value); }
        __synchronize(record) {
            this.__namespaceURI = record.namespace;
            this.__prefix = record.prefix;
            this.__localName = record.localName;
            this.__nodeName = record.qualifiedName;
            this.__value = record.value;
        }
        __attach(element) {
            this.__element = element;
            this.__document = element.ownerDocument;
        }
        __detach(value = this.__value) {
            this.__value = value;
            this.__element = null;
        }
    }
    Object.defineProperty(Attr.prototype, Symbol.toStringTag, { value: 'Attr', configurable: true });

    const createDetachedAttribute = (ownerDocument, namespace, prefix, localName, value = '') =>
        new Attr(attributeConstructionToken, ownerDocument, {
            namespace, prefix, localName, qualifiedName: qualifiedAttributeName(prefix, localName), value
        });
    const attrForRecord = (element, record) => {
        const state = cacheForAttributes(element);
        const key = attributeKey(record.namespace, record.localName);
        let attribute = state.attributes.get(key);
        if (!attribute || attribute.ownerElement !== element) {
            attribute = new Attr(attributeConstructionToken, element.ownerDocument, record, element);
            state.attributes.set(key, attribute);
        } else {
            attribute.__synchronize(record);
        }
        return attribute;
    };
    const snapshotAttributes = element => {
        const state = cacheForAttributes(element);
        const seen = new Set();
        const attributes = attributeRecords(element).map(record => {
            seen.add(attributeKey(record.namespace, record.localName));
            return attrForRecord(element, record);
        });
        for (const [key, attribute] of state.attributes) {
            if (!seen.has(key) && attribute.ownerElement === element) {
                attribute.__detach();
                state.attributes.delete(key);
            }
        }
        return attributes;
    };
    const recordByQualifiedName = (element, qualifiedName) => {
        qualifiedName = normalizedQualifiedName(element, qualifiedName);
        return attributeRecords(element).find(record => record.qualifiedName === qualifiedName) || null;
    };
    const recordByNamespace = (element, namespace, localName) => {
        namespace = normalizedNamespace(namespace);
        localName = String(localName);
        return attributeRecords(element).find(record =>
            record.namespace === namespace && record.localName === localName) || null;
    };
    const maybeRefreshNamedProperties = (element, namespace, localName) => {
        if (element.isConnected && namespace === null && (localName === 'id' || localName === 'name'))
            refreshWindowNamedProperties();
    };
    const queueAttributeMutation = (element, record, oldValue) => queueMutationRecord(element, 'attributes', {
        attributeName: record.localName,
        attributeNamespace: record.namespace,
        oldValue
    });
    const detachAttribute = (element, record, attribute) => {
        cacheForAttributes(element).attributes.delete(attributeKey(record.namespace, record.localName));
        attribute.__detach(record.value);
    };
    const setAttachedAttributeValue = (attribute, value) => {
        const element = attribute.ownerElement;
        const oldValue = attribute.value;
        host('attrSetNs', element.__id, attribute.namespaceURI || '', attribute.prefix || '', attribute.localName, value);
        attribute.__value = value;
        queueAttributeMutation(element, {
            namespace: attribute.namespaceURI, localName: attribute.localName
        }, oldValue);
        maybeRefreshNamedProperties(element, attribute.namespaceURI, attribute.localName);
    };

    class NamedNodeMap {
        constructor(token, element) {
            if (token !== attributeConstructionToken) throw new TypeError('Illegal constructor');
            namedNodeMapElements.set(this, element);
        }
        get length() { return snapshotAttributes(namedNodeMapElements.get(this)).length; }
        item(index) { return snapshotAttributes(namedNodeMapElements.get(this))[Number(index) >>> 0] || null; }
        getNamedItem(qualifiedName) {
            const element = namedNodeMapElements.get(this);
            const record = recordByQualifiedName(element, qualifiedName);
            return record ? attrForRecord(element, record) : null;
        }
        getNamedItemNS(namespace, localName) {
            const element = namedNodeMapElements.get(this);
            const record = recordByNamespace(element, namespace, localName);
            return record ? attrForRecord(element, record) : null;
        }
        setNamedItem(attribute) { return setAttributeNodeFor(namedNodeMapElements.get(this), attribute); }
        setNamedItemNS(attribute) { return setAttributeNodeFor(namedNodeMapElements.get(this), attribute); }
        removeNamedItem(qualifiedName) {
            const attribute = this.getNamedItem(qualifiedName);
            if (!attribute) throw new DOMException('The attribute was not found', 'NotFoundError');
            return removeAttributeNodeFor(namedNodeMapElements.get(this), attribute);
        }
        removeNamedItemNS(namespace, localName) {
            const attribute = this.getNamedItemNS(namespace, localName);
            if (!attribute) throw new DOMException('The attribute was not found', 'NotFoundError');
            return removeAttributeNodeFor(namedNodeMapElements.get(this), attribute);
        }
    }
    Object.defineProperty(NamedNodeMap.prototype, Symbol.toStringTag, { value: 'NamedNodeMap', configurable: true });

    const arrayIndex = property => {
        if (typeof property !== 'string' || property === '') return null;
        const index = Number(property);
        return Number.isInteger(index) && index >= 0 && index < 0xffffffff && String(index) === property
            ? index : null;
    };
    const supportedAttributeNames = element => {
        const htmlNames = usesHtmlAttributeNames(element);
        return [...new Set(attributeRecords(element).map(record => record.qualifiedName))]
            .filter(name => !htmlNames || name === name.toLowerCase());
    };
    const namedPropertyAttribute = (target, property) => {
        const element = namedNodeMapElements.get(target);
        return supportedAttributeNames(element).includes(property) ? target.getNamedItem(property) : null;
    };
    const namedNodeMapProxy = collection => {
        const proxy = new Proxy(collection, {
        get(target, property, receiver) {
            const index = arrayIndex(property);
            if (index !== null) return target.item(index) || undefined;
            if (Reflect.has(target, property)) return Reflect.get(target, property, receiver);
            return typeof property === 'string' ? namedPropertyAttribute(target, property) || undefined : undefined;
        },
        has(target, property) {
            const index = arrayIndex(property);
            if (index !== null) return index < target.length;
            return Reflect.has(target, property) ||
                (typeof property === 'string' && namedPropertyAttribute(target, property) !== null);
        },
        ownKeys(target) {
            const element = namedNodeMapElements.get(target);
            const attributes = snapshotAttributes(element);
            const keys = attributes.map((_, index) => String(index));
            for (const name of supportedAttributeNames(element))
                if (!(name in target) && !keys.includes(name)) keys.push(name);
            for (const key of Reflect.ownKeys(target)) if (!keys.includes(key)) keys.push(key);
            return keys;
        },
        getOwnPropertyDescriptor(target, property) {
            const index = arrayIndex(property);
            if (index !== null && index < target.length)
                return { configurable: true, enumerable: true, writable: false, value: target.item(index) };
            if (typeof property === 'string' && !(property in target)) {
                const attribute = namedPropertyAttribute(target, property);
                if (attribute) return { configurable: true, enumerable: false, writable: false, value: attribute };
            }
            return Reflect.getOwnPropertyDescriptor(target, property);
        }
        });
        namedNodeMapElements.set(proxy, namedNodeMapElements.get(collection));
        return proxy;
    };
    const attributeMapFor = element => {
        const state = cacheForAttributes(element);
        return state.collection ||= namedNodeMapProxy(new NamedNodeMap(attributeConstructionToken, element));
    };

    const createAttributeFor = (ownerDocument, localName) => {
        localName = validateAttributeLocalName(String(localName));
        if (host('isHtmlDocument', ownerDocument.__id)) localName = localName.toLowerCase();
        return createDetachedAttribute(ownerDocument, null, null, localName);
    };
    const createAttributeNsFor = (ownerDocument, namespace, qualifiedName) => {
        const extracted = validateAndExtractAttributeName(namespace, qualifiedName);
        return createDetachedAttribute(ownerDocument, extracted.namespace, extracted.prefix, extracted.localName);
    };
    const getAttributeNodeFor = (element, qualifiedName) => attributeMapFor(element).getNamedItem(qualifiedName);
    const getAttributeNodeNsFor = (element, namespace, localName) =>
        attributeMapFor(element).getNamedItemNS(namespace, localName);
    const setAttributeNodeFor = (element, attribute) => {
        if (!(attribute instanceof Attr)) throw new TypeError('The provided value is not an Attr');
        if (attribute.ownerElement !== null && attribute.ownerElement !== element)
            throw new DOMException('The attribute is already in use', 'InUseAttributeError');
        const oldAttribute = getAttributeNodeNsFor(element, attribute.namespaceURI, attribute.localName);
        if (oldAttribute === attribute) return attribute;
        const oldValue = oldAttribute?.value ?? null;
        host('attrReplaceNs', element.__id, attribute.namespaceURI || '', attribute.prefix || '',
            attribute.localName, attribute.value);
        if (oldAttribute) detachAttribute(element, {
            namespace: oldAttribute.namespaceURI, localName: oldAttribute.localName, value: oldValue
        }, oldAttribute);
        attribute.__attach(element);
        cacheForAttributes(element).attributes.set(attributeKey(attribute.namespaceURI, attribute.localName), attribute);
        queueAttributeMutation(element, {
            namespace: attribute.namespaceURI, localName: attribute.localName
        }, oldValue);
        maybeRefreshNamedProperties(element, attribute.namespaceURI, attribute.localName);
        return oldAttribute;
    };
    const removeAttributeNodeFor = (element, attribute) => {
        if (!(attribute instanceof Attr) || attribute.ownerElement !== element ||
            getAttributeNodeNsFor(element, attribute.namespaceURI, attribute.localName) !== attribute)
            throw new DOMException('The attribute was not found', 'NotFoundError');
        const record = {
            namespace: attribute.namespaceURI,
            prefix: attribute.prefix,
            localName: attribute.localName,
            qualifiedName: attribute.name,
            value: attribute.value
        };
        host('attrRemoveNs', element.__id, record.namespace || '', record.localName);
        detachAttribute(element, record, attribute);
        queueAttributeMutation(element, record, record.value);
        maybeRefreshNamedProperties(element, record.namespace, record.localName);
        return attribute;
    };
