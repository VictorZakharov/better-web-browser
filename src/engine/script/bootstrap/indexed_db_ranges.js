    // Key ranges use IndexedDB's cross-type ordering, not JavaScript comparison.
    // https://w3c.github.io/IndexedDB/#key-range
    const idbRangeToken = Symbol('IDBKeyRange');
    class IDBKeyRange {
        constructor(token, lower, upper, lowerOpen, upperOpen) {
            if (token !== idbRangeToken)
                throw new TypeError('Illegal constructor');
            this._lower = lower;
            this._upper = upper;
            this.lowerOpen = Boolean(lowerOpen);
            this.upperOpen = Boolean(upperOpen);
        }
        get lower() { return this._lower === null ? undefined : idbKeyValue(this._lower); }
        get upper() { return this._upper === null ? undefined : idbKeyValue(this._upper); }
        static only(value) {
            const key = idbKey(value);
            return new IDBKeyRange(idbRangeToken, key, key, false, false);
        }
        static lowerBound(value, open = false) {
            return new IDBKeyRange(idbRangeToken, idbKey(value), null, open, false);
        }
        static upperBound(value, open = false) {
            return new IDBKeyRange(idbRangeToken, null, idbKey(value), false, open);
        }
        static bound(lower, upper, lowerOpen = false, upperOpen = false) {
            const first = idbKey(lower), last = idbKey(upper);
            if (idbCompare(first, last) > 0 ||
                (idbCompare(first, last) === 0 && (lowerOpen || upperOpen)))
                throw new DOMException('Key range is empty', 'DataError');
            return new IDBKeyRange(idbRangeToken, first, last, lowerOpen, upperOpen);
        }
        includes(value) {
            const key = idbKey(value);
            return (this._lower === null || (this.lowerOpen ?
                idbCompare(key, this._lower) > 0 : idbCompare(key, this._lower) >= 0)) &&
                (this._upper === null || (this.upperOpen ?
                    idbCompare(key, this._upper) < 0 : idbCompare(key, this._upper) <= 0));
        }
    }
    const idbRange = query => {
        if (query === undefined || query === null) return null;
        const range = query instanceof IDBKeyRange ? query : IDBKeyRange.only(query);
        return {
            lower: range._lower, upper: range._upper,
            lowerOpen: range.lowerOpen, upperOpen: range.upperOpen
        };
    };
    globalThis.IDBKeyRange = IDBKeyRange;
