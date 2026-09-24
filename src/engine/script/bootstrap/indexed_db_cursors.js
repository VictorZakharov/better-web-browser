    // Each advancement asks the browser for one ordered record. A cursor does
    // not ship the entire store across IPC, and later writes remain observable.
    class IDBCursor {
        constructor(request, source, range, direction, keysOnly) {
            this.source = source;
            this.direction = direction;
            this._request = request;
            this._range = range;
            this._reverse = direction.startsWith('prev');
            this._keysOnly = keysOnly;
            this._record = null;
            this._value = undefined;
            this._exhausted = false;
        }
        get key() { return this._record ? idbKeyValue(this._record.key) : undefined; }
        get primaryKey() {
            return this._record ? idbKeyValue(this._record.primaryKey ?? this._record.key) : undefined;
        }
        _setRecord(record) {
            this._record = record;
            this._exhausted = record === null;
            this._value = record?.value === null || record?.value === undefined
                ? undefined : __deserializeClone(record.value);
            return record === null ? null : this;
        }
        _step(after, inclusive, skip, explicitKey = false, explicitPrimary = null) {
            if (!this.source.transaction._active)
                throw new DOMException('Transaction has finished', 'TransactionInactiveError');
            if (this._exhausted || !this._record)
                throw new DOMException('Cursor has no current record', 'InvalidStateError');
            if (this._request.readyState === 'pending')
                throw new DOMException('Cursor is already advancing', 'InvalidStateError');
            this._request._ready = false;
            const request = this._request;
            const operation = this.source._scanOperation
                ? this.source._scanOperation(this._range, after,
                    explicitPrimary ?? (explicitKey ? null : this._record.primaryKey),
                    inclusive, skip, this._reverse, this.direction.endsWith('unique'), this._keysOnly)
                : { kind: 'scan', store: this.source.name, range: this._range,
                    after, inclusive, skip, reverse: this._reverse, keysOnly: this._keysOnly };
            this.source.transaction._enqueue(this.source, operation,
                result => this._setRecord(result.Record), request);
        }
        continue(key) {
            let after = this._record?.key;
            let inclusive = false;
            if (key !== undefined) {
                after = idbKey(key);
                if (this._record && (this._reverse ?
                    idbCompare(after, this._record.key) >= 0 :
                    idbCompare(after, this._record.key) <= 0))
                    throw new DOMException('Key does not advance the cursor', 'DataError');
                inclusive = true;
            }
            this._step(after, inclusive, 0, key !== undefined);
        }
        continuePrimaryKey(key, primaryKey) {
            if (!(this.source instanceof IDBIndex) || this.direction.endsWith('unique'))
                throw new DOMException('continuePrimaryKey requires a non-unique index cursor',
                    'InvalidAccessError');
            if (arguments.length < 2) throw new TypeError('Both keys are required');
            const target = idbKey(key), primary = idbKey(primaryKey);
            if (this._record) {
                const order = idbCompare(target, this._record.key) ||
                    idbCompare(primary, this._record.primaryKey);
                if (this._reverse ? order >= 0 : order <= 0)
                    throw new DOMException('Keys do not advance the cursor', 'DataError');
            }
            this._step(target, true, 0, false, primary);
        }
        advance(count) {
            if (!Number.isInteger(Number(count)) || Number(count) < 1 ||
                Number(count) > 0xffffffff)
                throw new TypeError('advance count must be a positive unsigned long');
            this._step(this._record?.key, false, Number(count) - 1);
        }
        update(value) {
            if (this._keysOnly)
                throw new DOMException('Key cursor cannot update a value', 'InvalidStateError');
            if (!this._record)
                throw new DOMException('Cursor has no current record', 'InvalidStateError');
            const store = this.source.objectStore ?? this.source;
            if (store.keyPath === null) return store.put(value, this.primaryKey);
            const inline = idbPathValue(value, store.keyPath);
            if (inline === undefined || idbCompare(idbKey(inline),
                this._record.primaryKey ?? this._record.key) !== 0)
                throw new DOMException('Updated value changes the primary key', 'DataError');
            return store.put(value);
        }
        delete() {
            if (!this._record)
                throw new DOMException('Cursor has no current record', 'InvalidStateError');
            return (this.source.objectStore ?? this.source).delete(this.primaryKey);
        }
    }
    class IDBCursorWithValue extends IDBCursor {
        get value() { return this._value; }
    }
    globalThis.IDBCursor = IDBCursor;
    globalThis.IDBCursorWithValue = IDBCursorWithValue;
