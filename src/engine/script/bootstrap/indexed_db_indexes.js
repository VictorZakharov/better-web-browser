    // Index handles share the owning upgrade/transaction lifetime. The browser
    // derives and validates keys from stored clones; page-provided projections
    // are never trusted for uniqueness or reads.
    // https://w3c.github.io/IndexedDB/#idbindex
    const idbIndexPath = path => {
        if (Array.isArray(path)) {
            if (path.length === 0) throw new DOMException('Empty compound key path', 'SyntaxError');
            return path.map(idbIndexPathPart);
        }
        return idbIndexPathPart(path);
    };
    const idbIndexIdentifier = /^[$_\p{ID_Start}][$_\u200c\u200d\p{ID_Continue}]*$/u;
    const idbIndexPathPart = path => {
        path = String(path);
        if (path !== '' && path.split('.').some(part =>
            !idbIndexIdentifier.test(part)))
            throw new DOMException('Invalid index key path', 'SyntaxError');
        return path;
    };
    Object.defineProperty(IDBObjectStore.prototype, 'indexNames', {
        get() { return new DOMStringList(this._definition.indexes.map(index => index.name)); },
        enumerable: true
    });
    IDBObjectStore.prototype.createIndex = function(name, keyPath, options = {}) {
        const transaction = this.transaction;
        if (!transaction._upgrade)
            throw new DOMException('Indexes require a versionchange transaction', 'InvalidStateError');
        if (!transaction._active)
            throw new DOMException('Transaction has finished', 'TransactionInactiveError');
        if (!transaction.db._stores.has(this.name))
            throw new DOMException('Object store was deleted', 'InvalidStateError');
        name = String(name);
        if (this._definition.indexes.some(index => index.name === name))
            throw new DOMException('Index already exists', 'ConstraintError');
        const path = idbIndexPath(keyPath);
        const unique = Boolean(options.unique), multiEntry = Boolean(options.multiEntry);
        if (Array.isArray(path) && multiEntry)
            throw new DOMException('Compound multiEntry key path', 'InvalidAccessError');
        const definition = { name, keyPath: path, unique, multiEntry };
        this._definition.indexes.push(definition);
        return new IDBIndex(this, definition);
    };
    IDBObjectStore.prototype.deleteIndex = function(name) {
        const transaction = this.transaction;
        if (!transaction._upgrade)
            throw new DOMException('Indexes require a versionchange transaction', 'InvalidStateError');
        if (!transaction._active)
            throw new DOMException('Transaction has finished', 'TransactionInactiveError');
        name = String(name);
        const position = this._definition.indexes.findIndex(index => index.name === name);
        if (position < 0) throw new DOMException('Index does not exist', 'NotFoundError');
        this._definition.indexes.splice(position, 1);
    };
    IDBObjectStore.prototype.index = function(name) {
        if (!this.transaction._active)
            throw new DOMException('Transaction has finished', 'TransactionInactiveError');
        if (!this.transaction.db._stores.has(this.name))
            throw new DOMException('Object store was deleted', 'InvalidStateError');
        name = String(name);
        const definition = this._definition.indexes.find(index => index.name === name);
        if (!definition) throw new DOMException('Index does not exist', 'NotFoundError');
        return new IDBIndex(this, definition);
    };

    class IDBIndex {
        constructor(store, definition) {
            this.objectStore = store;
            this.transaction = store.transaction;
            this.name = definition.name;
            this.keyPath = Array.isArray(definition.keyPath)
                ? [...definition.keyPath] : definition.keyPath;
            this.multiEntry = definition.multiEntry;
            this.unique = definition.unique;
        }
        _operation(kind, query, extra = {}) {
            if (!this.transaction._active)
                throw new DOMException('Transaction has finished', 'TransactionInactiveError');
            return { kind, store: this.objectStore.name, index: this.name,
                range: idbRange(query), ...extra };
        }
        get(query) {
            if (query === undefined || query === null) idbInvalidKey();
            return this.transaction._enqueue(this, this._operation('indexGet', query,
                { keysOnly: false }), idbDecode);
        }
        getKey(query) {
            if (query === undefined || query === null) idbInvalidKey();
            return this.transaction._enqueue(this, this._operation('indexGet', query,
                { keysOnly: true }), result => result.Keys.length
                    ? idbKeyValue(result.Keys[0]) : undefined);
        }
        count(query) {
            return this.transaction._enqueue(this, this._operation('indexCount', query),
                result => result.Count);
        }
        getAll(query, count) {
            const limit = idbIndexLimit(count);
            return this.transaction._enqueue(this,
                this._operation('indexGetAll', query, { limit, keysOnly: false }),
                result => result.Values.map(__deserializeClone));
        }
        getAllKeys(query, count) {
            const limit = idbIndexLimit(count);
            return this.transaction._enqueue(this,
                this._operation('indexGetAll', query, { limit, keysOnly: true }),
                result => result.Keys.map(idbKeyValue));
        }
        _scanOperation(range, after, afterPrimary, inclusive, skip, reverse, unique, keysOnly) {
            return { kind: 'indexScan', store: this.objectStore.name, index: this.name,
                range, after, afterPrimary, inclusive, skip, reverse, unique, keysOnly };
        }
        _openCursor(query, direction, keysOnly) {
            if (!['next', 'nextunique', 'prev', 'prevunique'].includes(direction))
                throw new TypeError('Invalid cursor direction');
            const range = idbRange(query), reverse = direction.startsWith('prev');
            let request;
            request = this.transaction._enqueue(this, this._scanOperation(range,
                null, null, false, 0, reverse, direction.endsWith('unique'), keysOnly),
            result => {
                if (result.Record === null) return null;
                const Cursor = keysOnly ? IDBCursor : IDBCursorWithValue;
                return new Cursor(request, this, range, direction, keysOnly)
                    ._setRecord(result.Record);
            });
            return request;
        }
        openCursor(query, direction = 'next') {
            return this._openCursor(query, String(direction), false);
        }
        openKeyCursor(query, direction = 'next') {
            return this._openCursor(query, String(direction), true);
        }
    }
    const idbIndexLimit = count => {
        if (count === undefined) return null;
        const number = Number(count);
        if (!Number.isInteger(number) || number < 0 || number > 0xffffffff)
            throw new TypeError('count must be an unsigned long');
        return number;
    };
    globalThis.IDBIndex = IDBIndex;
