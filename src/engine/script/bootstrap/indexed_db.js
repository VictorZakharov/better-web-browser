    class IDBDatabase extends EventTarget {
        constructor(name, version, definitions, previousVersion) {
            super();
            this.name = name;
            this.version = version;
            this.onversionchange = null;
            this.onclose = null;
            this.onabort = null;
            this.onerror = null;
            this._previousVersion = previousVersion;
            this._stores = new Map(definitions.map(definition => [definition.name, definition]));
            this._upgradeTransaction = null;
            this._closed = false;
        }
        get objectStoreNames() { return new DOMStringList(this._stores.keys()); }
        createObjectStore(name, options = {}) {
            const transaction = this._upgradeTransaction;
            if (!transaction || !transaction._active)
                throw new DOMException('No active versionchange transaction', 'InvalidStateError');
            name = String(name);
            if (this._stores.has(name))
                throw new DOMException('Object store already exists', 'ConstraintError');
            const keyPath = options.keyPath === undefined || options.keyPath === null
                ? null : String(options.keyPath);
            if (keyPath !== null && (keyPath.length === 0 || keyPath.split('.').some(part =>
                !/^[A-Za-z_$][\w$]*$/.test(part))))
                throw new DOMException('Invalid object-store key path', 'SyntaxError');
            const autoIncrement = Boolean(options.autoIncrement);
            const definition = { name, keyPath, autoIncrement, indexes: [] };
            this._stores.set(name, definition);
            transaction._created.push(definition);
            transaction.objectStoreNames = new DOMStringList(this._stores.keys());
            return new IDBObjectStore(transaction, definition);
        }
        deleteObjectStore(name) {
            const transaction = this._upgradeTransaction;
            if (!transaction || !transaction._active)
                throw new DOMException('No active versionchange transaction', 'InvalidStateError');
            name = String(name);
            if (!this._stores.delete(name))
                throw new DOMException('Object store does not exist', 'NotFoundError');
            const created = transaction._created.findIndex(store => store.name === name);
            if (created >= 0) transaction._created.splice(created, 1);
            else transaction._removed.push(name);
            transaction.objectStoreNames = new DOMStringList(this._stores.keys());
        }
        transaction(storeNames, mode = 'readonly') {
            if (this._closed) throw new DOMException('Database is closed', 'InvalidStateError');
            if (mode !== 'readonly' && mode !== 'readwrite')
                throw new TypeError('Invalid transaction mode');
            const names = typeof storeNames === 'string' ? [storeNames] : Array.from(storeNames);
            if (names.length === 0)
                throw new DOMException('Transaction scope is empty', 'InvalidAccessError');
            for (const name of names)
                if (!this._stores.has(String(name)))
                    throw new DOMException('Object store does not exist', 'NotFoundError');
            return new IDBTransaction(this, names.map(String), mode);
        }
        close() { this._closed = true; }
    }
    class IDBFactory {
        databases() {
            return new Promise((resolve, reject) => {
                idbSend(null, { kind: 'list' }, response => {
                    if (response.kind === 'error') reject(idbError(response));
                    else if (response.kind === 'list') resolve(response.databases);
                    else reject(new DOMException('Malformed IndexedDB list response', 'UnknownError'));
                });
            });
        }
        open(name, version) {
            if (arguments.length === 0) throw new TypeError('Database name is required');
            name = String(name);
            if (version !== undefined && (!Number.isSafeInteger(Number(version)) || Number(version) < 1))
                throw new TypeError('Database version must be a positive integer');
            const requested = version === undefined ? null : Number(version);
            const request = new IDBOpenDBRequest();
            idbSend(request, { kind: 'open', name }, response => {
                if (response.kind === 'error') return request._fail(idbError(response));
                if (response.kind !== 'open') return request._fail(idbError({
                    name: 'UnknownError', message: 'Malformed IndexedDB open response'
                }));
                const info = response.info;
                const oldVersion = info?.version ?? 0;
                const targetVersion = requested ?? Math.max(1, oldVersion);
                if (targetVersion < oldVersion)
                    return request._fail(new DOMException('Database version is newer', 'VersionError'));
                const db = new IDBDatabase(name, targetVersion, info?.stores ?? [], oldVersion);
                request._result = db;
                if (targetVersion === oldVersion) return request._succeed(db);
                const transaction = new IDBTransaction(db, db.objectStoreNames, 'versionchange', true);
                db._upgradeTransaction = transaction;
                request.transaction = transaction;
                request._upgrade = true;
                transaction._onfinish = error => {
                    db._upgradeTransaction = null;
                    request.transaction = null;
                    if (error) request._fail(error);
                    else request._succeed(db);
                };
                try {
                    idbFire(request, 'upgradeneeded', new IDBVersionChangeEvent(
                        'upgradeneeded', { oldVersion, newVersion: targetVersion }), true);
                } catch (error) { transaction.abort(); }
            });
            return request;
        }
        deleteDatabase(name) {
            if (arguments.length === 0) throw new TypeError('Database name is required');
            const request = new IDBOpenDBRequest();
            idbSend(request, { kind: 'delete', name: String(name) }, response => {
                if (response.kind === 'error') request._fail(idbError(response));
                else if (response.kind === 'delete') request._succeed(undefined);
                else request._fail(idbError({
                    name: 'UnknownError', message: 'Malformed IndexedDB delete response'
                }));
            });
            return request;
        }
        cmp(first, second) { return idbCompare(idbKey(first), idbKey(second)); }
    }
    globalThis.IDBDatabase = IDBDatabase;
    globalThis.IDBFactory = IDBFactory;
    globalThis.indexedDB = new IDBFactory();
    globalThis.__receiveDatabaseEvent = (id, payload) => {
        const pending = pendingDatabaseRequests.get(Number(id));
        if (!pending) return;
        pendingDatabaseRequests.delete(Number(id));
        let response;
        try { response = JSON.parse(String(payload)); }
        catch (_) { response = { kind: 'error', name: 'UnknownError',
            message: 'Malformed IndexedDB response' }; }
        pending.callback(response);
    };
})();
