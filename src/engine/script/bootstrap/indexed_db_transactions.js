    // A transaction is active in its creation task and in request event handlers.
    // Flush at the microtask checkpoint so adjacent requests travel together.
    class IDBTransaction extends EventTarget {
        constructor(db, names, mode, upgrade = false) {
            super();
            this.db = db;
            this.mode = mode;
            this.durability = 'default';
            this.objectStoreNames = new DOMStringList(names);
            this.error = null;
            this.oncomplete = null;
            this.onabort = null;
            this.onerror = null;
            this._upgrade = upgrade;
            this._id = nextDatabaseTransactionId++;
            this._active = true;
            this._sent = false;
            this._begun = false;
            this._operations = [];
            this._requests = [];
            this._created = [];
            this._removed = [];
            this._onfinish = null;
            queueMicrotask(() => this._flush());
        }
        objectStore(name) {
            name = String(name);
            if (!this._active) throw new DOMException('Transaction has finished', 'InvalidStateError');
            if (!this.objectStoreNames.contains(name) || !this.db._stores.has(name))
                throw new DOMException('Object store is outside the transaction', 'NotFoundError');
            return new IDBObjectStore(this, this.db._stores.get(name));
        }
        abort() {
            if (!this._active) throw new DOMException('Transaction has finished', 'InvalidStateError');
            this._active = false;
            this.error = new DOMException('Transaction was aborted', 'AbortError');
            if (this._begun && !this._upgrade) idbSend(this, {
                kind: 'transaction', phase: 'abort', transactionId: this._id,
                name: this.db.name, version: this.db.version,
                mode: this.mode, operations: []
            }, () => {});
            for (const request of this._requests.splice(0))
                request._fail(this.error);
            idbFire(this, 'abort');
            if (this._onfinish) this._onfinish(this.error);
        }
        _enqueue(source, operation, result, existingRequest = null) {
            if (!this._active) throw new DOMException('Transaction has finished', 'TransactionInactiveError');
            if (this.mode === 'readonly' && ['put', 'delete', 'deleteRange', 'clear'].includes(operation.kind))
                throw new DOMException('Transaction is read-only', 'ReadOnlyError');
            const request = existingRequest ?? new IDBRequest(source, this);
            this._operations.push(operation);
            this._requests.push({ request, result });
            if (!this._sent) queueMicrotask(() => this._flush());
            return request;
        }
        _flush() {
            if (!this._active || this._sent) return;
            if (!this._upgrade && this._operations.length === 0) {
                this._finish();
                return;
            }
            const operations = this._operations.splice(0);
            const requests = this._requests.splice(0);
            this._sent = true;
            const command = this._upgrade ? {
                kind: 'upgrade', name: this.db.name,
                previousVersion: this.db._previousVersion, version: this.db.version,
                create: this._created, remove: this._removed, writes: operations,
                definitions: [...this.db._stores.values()]
            } : {
                kind: 'transaction', name: this.db.name, version: this.db.version,
                transactionId: this._id, phase: 'step',
                mode: this.mode, operations
            };
            if (!this._upgrade) this._begun = true;
            idbSend(this, command, response => {
                this._sent = false;
                if (!this._active) return;
                if (response.kind === 'error') {
                    this.error = idbError(response);
                    this._active = false;
                    for (const item of requests) item.request._fail(this.error);
                    idbFire(this, 'error', new Event('error', { bubbles: true }));
                    idbFire(this, 'abort');
                    if (this._onfinish) this._onfinish(this.error);
                    return;
                }
                if (!Array.isArray(response.results) || response.results.length !== requests.length) {
                    this.error = new DOMException('Malformed IndexedDB response', 'UnknownError');
                    this._active = false;
                    for (const item of requests) item.request._fail(this.error);
                    idbFire(this, 'abort');
                    if (this._onfinish) this._onfinish(this.error);
                    return;
                }
                for (let index = 0; index < requests.length; index++) {
                    const { request, result } = requests[index];
                    try { request._succeed(result(response.results[index])); }
                    catch (error) { request._fail(error); }
                }
                if (this._operations.length) queueMicrotask(() => this._flush());
                else this._finish();
            });
        }
        _finish() {
            if (!this._active) return;
            this._active = false;
            if (!this._upgrade && this._begun) {
                idbSend(this, {
                    kind: 'transaction', phase: 'commit', transactionId: this._id,
                    name: this.db.name, version: this.db.version,
                    mode: this.mode, operations: []
                }, response => {
                    if (response.kind === 'error') {
                        this.error = idbError(response);
                        idbFire(this, 'error', new Event('error', { bubbles: true }));
                        idbFire(this, 'abort');
                        if (this._onfinish) this._onfinish(this.error);
                    } else {
                        idbFire(this, 'complete');
                        if (this._onfinish) this._onfinish(null);
                    }
                });
            } else {
                idbFire(this, 'complete');
                if (this._onfinish) this._onfinish(null);
            }
        }
    }
    const idbPathValue = (value, path) => {
        let current = value;
        for (const part of path.split('.')) {
            if (current === null || (typeof current !== 'object' && typeof current !== 'function') ||
                !(part in current)) return undefined;
            current = current[part];
        }
        return current;
    };
    const idbDecode = result => result && Object.hasOwn(result, 'Value') &&
        result.Value !== null ? __deserializeClone(result.Value) : undefined;
    class IDBObjectStore {
        constructor(transaction, definition) {
            this.transaction = transaction;
            this.name = definition.name;
            this.keyPath = definition.keyPath;
            this.autoIncrement = definition.autoIncrement;
            this._definition = definition;
        }
        _put(value, key, overwrite) {
            if (this.transaction.mode === 'readonly')
                throw new DOMException('Transaction is read-only', 'ReadOnlyError');
            if (this.keyPath !== null && key !== undefined)
                throw new DOMException('An inline key cannot be supplied separately', 'DataError');
            const found = this.keyPath === null ? key : idbPathValue(value, this.keyPath);
            const recordKey = found === undefined ? null : idbKey(found);
            if (recordKey === null && !this.autoIncrement) idbInvalidKey();
            const serialized = __serializeClone(value);
            return this.transaction._enqueue(this, {
                kind: 'put', store: this.name, key: recordKey, value: serialized, overwrite
            }, result => idbKeyValue(result.Key));
        }
        put(value, key) { return this._put(value, key, true); }
        add(value, key) { return this._put(value, key, false); }
        get(key) {
            if (key instanceof IDBKeyRange)
                return this.transaction._enqueue(this, {
                    kind: 'scan', store: this.name, range: idbRange(key),
                    after: null, inclusive: false, skip: 0, reverse: false, keysOnly: false
                }, result => result.Record?.value == null ?
                    undefined : __deserializeClone(result.Record.value));
            const recordKey = idbKey(key);
            return this.transaction._enqueue(this, {
                kind: 'get', store: this.name, key: recordKey
            }, idbDecode);
        }
        getKey(query) {
            const range = idbRange(query);
            if (range === null) idbInvalidKey();
            return this.transaction._enqueue(this, {
                kind: 'scan', store: this.name, range,
                after: null, inclusive: false, skip: 0, reverse: false, keysOnly: true
            }, result => result.Record ? idbKeyValue(result.Record.key) : undefined);
        }
        delete(key) {
            if (key instanceof IDBKeyRange)
                return this.transaction._enqueue(this, {
                    kind: 'deleteRange', store: this.name, range: idbRange(key)
                }, () => undefined);
            const recordKey = idbKey(key);
            return this.transaction._enqueue(this, {
                kind: 'delete', store: this.name, key: recordKey
            }, () => undefined);
        }
        clear() {
            return this.transaction._enqueue(this, {
                kind: 'clear', store: this.name
            }, () => undefined);
        }
        count(query) {
            return this.transaction._enqueue(this, {
                kind: 'count', store: this.name, range: idbRange(query)
            }, result => result.Count);
        }
        getAll(query, count) {
            if (count !== undefined && (!Number.isInteger(Number(count)) ||
                Number(count) < 0 || Number(count) > 0xffffffff))
                throw new TypeError('count must be an unsigned long');
            return this.transaction._enqueue(this, {
                kind: 'getAll', store: this.name, range: idbRange(query),
                limit: count === undefined ? null : Number(count), keysOnly: false
            }, result => result.Values.map(__deserializeClone));
        }
        getAllKeys(query, count) {
            if (count !== undefined && (!Number.isInteger(Number(count)) ||
                Number(count) < 0 || Number(count) > 0xffffffff))
                throw new TypeError('count must be an unsigned long');
            return this.transaction._enqueue(this, {
                kind: 'getAll', store: this.name, range: idbRange(query),
                limit: count === undefined ? null : Number(count), keysOnly: true
            }, result => result.Keys.map(idbKeyValue));
        }
        _openCursor(query, direction, keysOnly) {
            if (!['next', 'nextunique', 'prev', 'prevunique'].includes(direction))
                throw new TypeError('Invalid cursor direction');
            const range = idbRange(query), reverse = direction.startsWith('prev');
            let request;
            request = this.transaction._enqueue(this, {
                kind: 'scan', store: this.name, range, after: null,
                inclusive: false, skip: 0, reverse, keysOnly
            }, result => {
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
    globalThis.IDBTransaction = IDBTransaction;
    globalThis.IDBObjectStore = IDBObjectStore;
