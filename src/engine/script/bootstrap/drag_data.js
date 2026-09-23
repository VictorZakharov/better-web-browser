    // HTML drag data store: event phases share items but expose different modes.
    // https://html.spec.whatwg.org/multipage/dnd.html#the-drag-data-store
    const dragStores = new WeakMap();
    const dataTransferItemToken = Symbol('DataTransferItem');
    const fileListToken = Symbol('FileList');
    const dragTypes = new Set([
        'none', 'copy', 'copyLink', 'copyMove', 'link', 'linkMove', 'move', 'all', 'uninitialized'
    ]);
    const dropTypes = new Set(['none', 'copy', 'link', 'move']);
    const dragFormat = value => {
        value = String(value).trim().replace(/[A-Z]/g, character => character.toLowerCase());
        if (value === 'text') return 'text/plain';
        if (value === 'url') return 'text/uri-list';
        return value;
    };
    const dragStore = transfer => {
        const store = dragStores.get(transfer);
        if (!store) throw new TypeError('Invalid DataTransfer receiver');
        return store;
    };
    const synchronizeIndexedItems = list => {
        for (const key of Object.keys(list)) if (/^(0|[1-9]\d*)$/.test(key)) delete list[key];
        const store = dragStore(list.__owner);
        if (store.mode === 'disabled') return;
        for (let index = 0; index < store.items.length; index++)
            Object.defineProperty(list, index, { configurable: true, enumerable: true,
                get: () => store.items[index]?.wrapper });
    };
    class FileList {
        constructor(token, files) {
            if (token !== fileListToken) throw new TypeError('Illegal constructor');
            for (let index = 0; index < files.length; index++)
                Object.defineProperty(this, index, { enumerable: true, value: files[index] });
            Object.defineProperty(this, 'length', { enumerable: true, value: files.length });
        }
        item(index) { return this[Number(index)] ?? null; }
        [Symbol.iterator]() { return Array.prototype[Symbol.iterator].call(this); }
    }
    class DataTransferItem {
        constructor(token, owner, entry) {
            if (token !== dataTransferItemToken) throw new TypeError('Illegal constructor');
            this.__owner = owner;
            this.__entry = entry;
        }
        get kind() {
            const store = dragStore(this.__owner);
            return store.mode !== 'disabled' && store.items.includes(this.__entry) ? this.__entry.kind : '';
        }
        get type() { return this.kind ? this.__entry.type : ''; }
        getAsString(callback) {
            const store = dragStore(this.__owner);
            if (typeof callback !== 'function' || this.kind !== 'string' || store.mode === 'protected') return;
            const value = this.__entry.data;
            setTimeout(() => callback(value), 0);
        }
        getAsFile() {
            const store = dragStore(this.__owner);
            return store.mode === 'protected' || this.kind !== 'file' ? null : this.__entry.data;
        }
    }
    class DataTransferItemList {
        constructor(owner) {
            this.__owner = owner;
            synchronizeIndexedItems(this);
        }
        get length() {
            const store = dragStore(this.__owner);
            return store.mode === 'disabled' ? 0 : store.items.length;
        }
        item(index) { return this[Number(index)] ?? null; }
        add(data, type) {
            const store = dragStore(this.__owner);
            if (store.mode !== 'readwrite') return null;
            const file = typeof File === 'function' && data instanceof File;
            if (!file && arguments.length < 2) throw new TypeError('String data requires a type');
            const format = file ? data.type.toLowerCase() : dragFormat(type);
            if (!file && store.items.some(item => item.kind === 'string' && item.type === format))
                throw new DOMException('Duplicate drag data type', 'NotSupportedError');
            const entry = { kind: file ? 'file' : 'string', type: format,
                data: file ? data : String(data), wrapper: null };
            entry.wrapper = new DataTransferItem(dataTransferItemToken, this.__owner, entry);
            store.items.push(entry);
            synchronizeIndexedItems(this);
            return entry.wrapper;
        }
        remove(index) {
            const store = dragStore(this.__owner);
            if (store.mode !== 'readwrite')
                throw new DOMException('Drag data is not writable', 'InvalidStateError');
            index = Number(index) >>> 0;
            if (index < store.items.length) store.items.splice(index, 1);
            synchronizeIndexedItems(this);
        }
        clear() {
            const store = dragStore(this.__owner);
            if (store.mode !== 'readwrite') return;
            store.items.length = 0;
            synchronizeIndexedItems(this);
        }
        [Symbol.iterator]() { return Array.from({length: this.length}, (_, index) => this[index])[Symbol.iterator](); }
    }
    class DataTransfer {
        constructor() {
            const store = { mode: 'readwrite', items: [], dropEffect: 'none',
                effectAllowed: 'none', image: null };
            dragStores.set(this, store);
            store.list = new DataTransferItemList(this);
        }
        get dropEffect() { return dragStore(this).dropEffect; }
        set dropEffect(value) { if (dropTypes.has(String(value))) dragStore(this).dropEffect = String(value); }
        get effectAllowed() { return dragStore(this).effectAllowed; }
        set effectAllowed(value) {
            const store = dragStore(this);
            if (store.mode === 'readwrite' && dragTypes.has(String(value))) store.effectAllowed = String(value);
        }
        get items() { return dragStore(this).list; }
        get types() {
            const store = dragStore(this);
            if (store.mode === 'disabled') return Object.freeze([]);
            const types = store.items.filter(item => item.kind === 'string').map(item => item.type);
            if (store.items.some(item => item.kind === 'file')) types.push('Files');
            return Object.freeze(types);
        }
        get files() {
            const store = dragStore(this);
            const files = store.mode === 'protected' || store.mode === 'disabled' ? [] :
                store.items.filter(item => item.kind === 'file').map(item => item.data);
            return new FileList(fileListToken, files);
        }
        setData(format, data) {
            const store = dragStore(this);
            if (store.mode !== 'readwrite') return;
            format = dragFormat(format);
            const old = store.items.findIndex(item => item.kind === 'string' && item.type === format);
            // Replacing an existing format keeps its position in the drag data store.
            if (old >= 0) store.items[old].data = String(data);
            else store.list.add(String(data), format);
        }
        getData(format) {
            const store = dragStore(this);
            if (store.mode === 'protected' || store.mode === 'disabled') return '';
            format = dragFormat(format);
            let value = store.items.find(item => item.kind === 'string' && item.type === format)?.data ?? '';
            if (format === 'text/uri-list' && String(arguments[0]).toLowerCase() === 'url')
                value = value.split(/\r?\n/).find(line => line && !line.startsWith('#')) ?? '';
            return value;
        }
        clearData(format) {
            const store = dragStore(this);
            if (store.mode !== 'readwrite') return;
            const type = arguments.length ? dragFormat(format) : null;
            store.items = store.items.filter(item => item.kind !== 'string' || (type && item.type !== type));
            synchronizeIndexedItems(store.list);
        }
        setDragImage(element, x, y) {
            if (!(element instanceof Element)) throw new TypeError('Drag image must be an Element');
            const store = dragStore(this);
            if (store.mode === 'readwrite') store.image = { element, x: Number(x), y: Number(y) };
        }
    }
    const setDragDataMode = (transfer, mode) => {
        const store = dragStore(transfer);
        store.mode = mode;
        synchronizeIndexedItems(store.list);
    };
