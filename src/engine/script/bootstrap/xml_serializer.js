    // DOM Parsing and Serialization: XMLSerializer's node tree is read natively so
    // author-overridden node accessors cannot change the serialized document.
    const xmlSerializers = new WeakSet();
    class XMLSerializer {
        constructor() { xmlSerializers.add(this); }
        serializeToString(root) {
            if (!xmlSerializers.has(this)) throw new TypeError('Illegal invocation');
            if (arguments.length < 1 || !isNode(root))
                throw new TypeError('serializeToString requires a Node');
            return host('xmlSerialize', nodeId(root));
        }
    }
    Object.defineProperty(XMLSerializer.prototype, Symbol.toStringTag,
        {value: 'XMLSerializer', configurable: true});
