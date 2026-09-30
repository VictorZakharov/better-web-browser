    // HTML drag data store: event phases share items but expose different modes.
    // https://html.spec.whatwg.org/multipage/dnd.html#the-drag-data-store
    const dragTransfers = new WeakMap();
    const dragLists = new WeakMap();
    const dragItems = new WeakMap();
    const fileLists = new WeakMap();
    const eventTransferToken = Symbol('event DataTransfer');
    const dataTransferItemToken = Symbol('DataTransferItem');
    const dataTransferItemListToken = Symbol('DataTransferItemList');
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
    const dragTransfer = transfer => {
        const state = dragTransfers.get(transfer);
        if (!state) throw new TypeError('Invalid DataTransfer receiver');
        return state;
    };
    const dragStore = transfer => dragTransfer(transfer).store;
    const dragList = list => {
        const state = dragLists.get(list);
        if (!state) throw new TypeError('Invalid DataTransferItemList receiver');
        return state;
    };
    const dragItemState = item => {
        const state = dragItems.get(item);
        if (!state) throw new TypeError('Invalid DataTransferItem receiver');
        return state;
    };
    const dragItem = (list, entry) => {
        const state = dragList(list);
        let item = state.items.get(entry);
        if (!item) {
            item = new DataTransferItem(dataTransferItemToken, state.owner, entry);
            state.items.set(entry, item);
        }
        return item;
    };
    const synchronizeIndexedItems = list => {
        for (const key of Object.keys(list)) if (/^(0|[1-9]\d*)$/.test(key)) delete list[key];
        const store = dragStore(dragList(list).owner);
        if (!store) return;
        for (let index = 0; index < store.items.length; index++)
            Object.defineProperty(list, index, { configurable: true, enumerable: true,
                get: () => store.items[index] ? dragItem(list, store.items[index]) : undefined });
    };
    const fileListContents = list => {
        const state = fileLists.get(list);
        if (!state) throw new TypeError('Invalid FileList receiver');
        if (!state.owner) return state.files;
        const store = dragStore(state.owner);
        return !store || store.mode === 'protected' ? [] :
            store.items.filter(item => item.kind === 'file').map(item => item.data);
    };
    const synchronizeFileList = list => {
        const state = fileLists.get(list);
        for (let index = 0; index < state.indexedLength; index++) delete list[index];
        const length = fileListContents(list).length;
        for (let index = 0; index < length; index++)
            Object.defineProperty(list, index, { configurable: true, enumerable: true,
                get: () => fileListContents(list)[index] });
        state.indexedLength = length;
        for (const observer of Array.from(state.observers)) observer();
    };
    const observeFileList = (list, observer) => {
        const observers = fileLists.get(list).observers;
        observers.add(observer);
        return () => observers.delete(observer);
    };
    const fileListForInputAssignment = list => {
        const owner = fileLists.get(list).owner;
        // An event drag data store expires after dispatch. Selected files must
        // outlive the drop event even though its DataTransfer.files goes empty.
        return owner && dragTransfer(owner).eventScoped ?
            new FileList(fileListToken, fileListContents(list).slice()) : list;
    };
    Object.defineProperty(globalThis, '__fileListContents', {
        configurable: true, value: fileListContents
    });
    const synchronizeTransferFiles = transfer => {
        const list = dragTransfer(transfer).files;
        if (list) synchronizeFileList(list);
    };
    class FileList {
        constructor(token, files, owner = null) {
            if (token !== fileListToken) throw new TypeError('Illegal constructor');
            fileLists.set(this, { files, owner, indexedLength: 0, observers: new Set() });
            Object.defineProperty(this, 'length', { enumerable: true,
                get: () => fileListContents(this).length });
            synchronizeFileList(this);
        }
        item(index) { return fileListContents(this)[Number(index)] ?? null; }
        [Symbol.iterator]() { return Array.prototype[Symbol.iterator].call(this); }
    }
    class DataTransferItem {
        constructor(token, owner, entry) {
            if (token !== dataTransferItemToken) throw new TypeError('Illegal constructor');
            dragItems.set(this, { owner, entry });
        }
        get kind() {
            const { owner, entry } = dragItemState(this);
            const store = dragStore(owner);
            return store && store.items.includes(entry) ? entry.kind : '';
        }
        get type() { return this.kind ? dragItemState(this).entry.type : ''; }
        getAsString(callback) {
            const { owner, entry } = dragItemState(this);
            const store = dragStore(owner);
            if (typeof callback !== 'function' || this.kind !== 'string' || store.mode === 'protected') return;
            const value = entry.data;
            setTimeout(() => callback(value), 0);
        }
        getAsFile() {
            const { owner, entry } = dragItemState(this);
            const store = dragStore(owner);
            return !store || store.mode === 'protected' || this.kind !== 'file' ? null : entry.data;
        }
    }
    class DataTransferItemList {
        constructor(token, owner) {
            if (token !== dataTransferItemListToken) throw new TypeError('Illegal constructor');
            dragLists.set(this, { owner, items: new WeakMap() });
            synchronizeIndexedItems(this);
        }
        get length() {
            const store = dragStore(dragList(this).owner);
            return store ? store.items.length : 0;
        }
        item(index) { return this[Number(index)] ?? null; }
        add(data, type) {
            const owner = dragList(this).owner;
            const store = dragStore(owner);
            if (!store || store.mode !== 'readwrite') return null;
            const file = typeof File === 'function' && data instanceof File;
            if (!file && arguments.length < 2) throw new TypeError('String data requires a type');
            const format = file ? data.type.toLowerCase() : dragFormat(type);
            if (!file && store.items.some(item => item.kind === 'string' && item.type === format))
                throw new DOMException('Duplicate drag data type', 'NotSupportedError');
            const entry = { kind: file ? 'file' : 'string', type: format,
                data: file ? data : String(data) };
            store.items.push(entry);
            synchronizeIndexedItems(this);
            if (file) synchronizeTransferFiles(owner);
            return dragItem(this, entry);
        }
        remove(index) {
            const owner = dragList(this).owner;
            const store = dragStore(owner);
            if (!store || store.mode !== 'readwrite')
                throw new DOMException('Drag data is not writable', 'InvalidStateError');
            index = Number(index) >>> 0;
            const removed = index < store.items.length ? store.items.splice(index, 1)[0] : null;
            synchronizeIndexedItems(this);
            if (removed?.kind === 'file') synchronizeTransferFiles(owner);
        }
        clear() {
            const owner = dragList(this).owner;
            const store = dragStore(owner);
            if (!store || store.mode !== 'readwrite') return;
            const hadFiles = store.items.some(item => item.kind === 'file');
            store.items.length = 0;
            synchronizeIndexedItems(this);
            if (hadFiles) synchronizeTransferFiles(owner);
        }
        [Symbol.iterator]() { return Array.from({length: this.length}, (_, index) => this[index])[Symbol.iterator](); }
    }
    class DataTransfer {
        constructor(token, eventStore) {
            const store = token === eventTransferToken ? eventStore :
                { mode: 'readwrite', items: [], effectAllowed: 'none', image: null };
            const state = { store, eventScoped: token === eventTransferToken,
                dropEffect: 'none', effectAllowed: store.effectAllowed,
                list: null, files: null };
            dragTransfers.set(this, state);
            state.list = new DataTransferItemList(dataTransferItemListToken, this);
        }
        get dropEffect() { return dragTransfer(this).dropEffect; }
        set dropEffect(value) {
            if (dropTypes.has(String(value))) dragTransfer(this).dropEffect = String(value);
        }
        get effectAllowed() { return dragTransfer(this).effectAllowed; }
        set effectAllowed(value) {
            const state = dragTransfer(this);
            if (state.store?.mode === 'readwrite' && dragTypes.has(String(value)))
                state.effectAllowed = String(value);
        }
        get items() { return dragTransfer(this).list; }
        get types() {
            const store = dragStore(this);
            if (!store) return Object.freeze([]);
            const types = store.items.filter(item => item.kind === 'string').map(item => item.type);
            if (store.items.some(item => item.kind === 'file')) types.push('Files');
            return Object.freeze(types);
        }
        get files() {
            const state = dragTransfer(this);
            return state.files ||= new FileList(fileListToken, null, this);
        }
        setData(format, data) {
            const store = dragStore(this);
            if (!store || store.mode !== 'readwrite') return;
            format = dragFormat(format);
            const old = store.items.findIndex(item => item.kind === 'string' && item.type === format);
            // Replacing an existing format keeps its position in the drag data store.
            if (old >= 0) store.items[old].data = String(data);
            else this.items.add(String(data), format);
        }
        getData(format) {
            const store = dragStore(this);
            if (!store || store.mode === 'protected') return '';
            format = dragFormat(format);
            let value = store.items.find(item => item.kind === 'string' && item.type === format)?.data ?? '';
            if (format === 'text/uri-list' && String(arguments[0]).toLowerCase() === 'url')
                value = value.split(/\r?\n/).find(line => line && !line.startsWith('#')) ?? '';
            return value;
        }
        clearData(format) {
            const store = dragStore(this);
            if (!store || store.mode !== 'readwrite') return;
            const type = arguments.length ? dragFormat(format) : null;
            store.items = store.items.filter(item => item.kind !== 'string' || (type && item.type !== type));
            synchronizeIndexedItems(this.items);
        }
        setDragImage(element, x, y) {
            if (!(element instanceof Element)) throw new TypeError('Drag image must be an Element');
            const store = dragStore(this);
            if (store?.mode === 'readwrite') store.image = { element, x: Number(x), y: Number(y) };
        }
    }
    const createDragDataStore = () => ({
        mode: 'protected', items: [], effectAllowed: 'uninitialized', image: null
    });
    const eventDataTransfer = (store, dropEffect) => {
        const transfer = new DataTransfer(eventTransferToken, store);
        dragTransfer(transfer).dropEffect = dropEffect;
        return transfer;
    };
    const detachEventDataTransfer = transfer => {
        const state = dragTransfer(transfer);
        state.store = null;
        synchronizeIndexedItems(state.list);
        if (state.files) synchronizeFileList(state.files);
    };
    const appendDefaultDragData = (store, format, data) => {
        store.items.push({ kind: 'string', type: format, data });
    };
