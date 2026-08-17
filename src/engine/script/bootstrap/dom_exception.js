(() => {
    'use strict';

    // Web IDL keeps these constants and codes for legacy web compatibility.
    const legacyCodes = Object.freeze({
        IndexSizeError: ['INDEX_SIZE_ERR', 1],
        HierarchyRequestError: ['HIERARCHY_REQUEST_ERR', 3],
        WrongDocumentError: ['WRONG_DOCUMENT_ERR', 4],
        InvalidCharacterError: ['INVALID_CHARACTER_ERR', 5],
        NoModificationAllowedError: ['NO_MODIFICATION_ALLOWED_ERR', 7],
        NotFoundError: ['NOT_FOUND_ERR', 8],
        NotSupportedError: ['NOT_SUPPORTED_ERR', 9],
        InUseAttributeError: ['INUSE_ATTRIBUTE_ERR', 10],
        InvalidStateError: ['INVALID_STATE_ERR', 11],
        SyntaxError: ['SYNTAX_ERR', 12],
        InvalidModificationError: ['INVALID_MODIFICATION_ERR', 13],
        NamespaceError: ['NAMESPACE_ERR', 14],
        InvalidAccessError: ['INVALID_ACCESS_ERR', 15],
        TypeMismatchError: ['TYPE_MISMATCH_ERR', 17],
        SecurityError: ['SECURITY_ERR', 18],
        NetworkError: ['NETWORK_ERR', 19],
        AbortError: ['ABORT_ERR', 20],
        URLMismatchError: ['URL_MISMATCH_ERR', 21],
        QuotaExceededError: ['QUOTA_EXCEEDED_ERR', 22],
        TimeoutError: ['TIMEOUT_ERR', 23],
        InvalidNodeTypeError: ['INVALID_NODE_TYPE_ERR', 24],
        DataCloneError: ['DATA_CLONE_ERR', 25],
    });

    const slots = new WeakMap();
    const slotFor = value => {
        const slot = slots.get(value);
        if (!slot) throw new TypeError('DOMException getter called on an incompatible receiver');
        return slot;
    };

    function DOMException(message = '', name = 'Error') {
        if (!new.target) throw new TypeError("DOMException requires 'new'");
        const instance = Reflect.construct(Error, [], new.target);
        const convertedMessage = String(message);
        const convertedName = String(name);
        slots.set(instance, {
            message: convertedMessage,
            name: convertedName,
            code: legacyCodes[convertedName]?.[1] || 0,
        });
        return instance;
    }

    Object.setPrototypeOf(DOMException.prototype, Error.prototype);
    Object.defineProperty(DOMException, 'prototype', { writable: false });
    for (const name of ['message', 'name', 'code']) {
        Object.defineProperty(DOMException.prototype, name, {
            get() { return slotFor(this)[name]; },
            enumerable: true,
            configurable: true,
        });
    }
    Object.defineProperty(DOMException.prototype, Symbol.toStringTag, {
        value: 'DOMException', configurable: true,
    });

    const constants = [
        ['DOMSTRING_SIZE_ERR', 2],
        ['NO_DATA_ALLOWED_ERR', 6],
        ['VALIDATION_ERR', 16],
        ...Object.values(legacyCodes),
    ];
    for (const [name, value] of constants) {
        const descriptor = { value, writable: false, enumerable: true, configurable: false };
        Object.defineProperty(DOMException, name, descriptor);
        Object.defineProperty(DOMException.prototype, name, descriptor);
    }
    Object.defineProperty(globalThis, 'DOMException', {
        value: DOMException, writable: true, configurable: true,
    });
})();
