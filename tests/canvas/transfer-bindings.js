// Shared Window/Worker transfer tests. Ownership changes follow transfer-list
// order, after graph serialization; failure is not an all-resource rollback.
function testCanvasTransfers() {
    const assert = (condition, label) => { if (!condition) throw Error(label); };
    const throws = (callback, name, label) => {
        let actual = '';
        try { callback(); } catch (error) { actual = error.name; }
        assert(actual === name, `${label}: ${actual}`);
    };
    for (const mode of ['2d', 'bitmaprenderer']) {
        const active = new OffscreenCanvas(2, 3), context = active.getContext(mode);
        const untouched = new OffscreenCanvas(4, 5);
        throws(() => structuredClone(null, {transfer:[active, untouched]}),
            'InvalidStateError', 'unused active canvas still runs transfer steps');
        assert(active.width === 2 && active.getContext(mode) === context,
            'rejected active resource retains ownership');
        assert(untouched.width === 4, 'later resource was not transferred');
        const earlier = new OffscreenCanvas(6, 7);
        throws(() => structuredClone(null, {transfer:[earlier, active]}),
            'InvalidStateError', 'later invalid resource rejects transfer');
        assert(earlier.width === 0, 'earlier successful transfer was detached');
        assert(active.width === 2, 'failing resource was not detached');
    }
    const mutable = new OffscreenCanvas(2, 3);
    const payload = {canvas:mutable, again:mutable, get resize() {mutable.width = 7; return true;}};
    const cloned = structuredClone(payload, {transfer:[mutable]});
    assert(cloned.canvas === cloned.again && cloned.canvas.width === 7,
        'transfer snapshots after getters and preserves graph identity');
    assert(mutable.width === 0, 'snapshot owner detached');
    const initialized = new OffscreenCanvas(2, 3);
    const initializer = {canvas:initialized, get initialize() {initialized.getContext('2d'); return 1;}};
    throws(() => structuredClone(initializer, {transfer:[initialized]}),
        'InvalidStateError', 'context created by later getter invalidates transfer');
    assert(initialized.width === 2, 'late-invalid owner retained');
    const active = new OffscreenCanvas(2, 3);active.getContext('2d');
    throws(() => structuredClone({canvas:active, get error() {throw new RangeError('getter');}},
        {transfer:[active]}), 'RangeError', 'graph exception precedes transfer exception');
    const unused = new OffscreenCanvas(2, 3);
    assert(structuredClone('message', {transfer:[unused]}) === 'message' && unused.width === 0,
        'unused valid resource is detached');
    const buffer = new Uint8Array([1,2,3]);
    const changed = structuredClone({buffer:buffer.buffer, get update() {buffer[0] = 9; return 1;}},
        {transfer:[buffer.buffer]});
    assert(new Uint8Array(changed.buffer)[0] === 9 && buffer.byteLength === 0,
        'buffer snapshot also runs after serialization');
    const hidden = new OffscreenCanvas(2, 3);
    Object.setPrototypeOf(hidden, null);
    const brandClone = structuredClone(hidden, {transfer:[hidden]});
    assert(brandClone.width === 2, 'transfer uses private brand rather than prototype');
    return {passed:true};
}

function testCanvasCloneIsolation() {
    const assert = (condition, label) => { if (!condition) throw Error(label); };
    let calls = 0;
    const oldObject = Object.getOwnPropertyDescriptor(Object.prototype, 'toJSON');
    const oldArray = Object.getOwnPropertyDescriptor(Array.prototype, 'toJSON');
    const oldHook = globalThis.__cloneCanvasBindings;
    const restore = (prototype, old) => {
        if (old) Object.defineProperty(prototype, 'toJSON', old);
        else delete prototype.toJSON;
    };
    try {
        Object.defineProperty(Object.prototype, 'toJSON', {
            configurable:true, value() {calls++; throw Error('author JSON hook');}
        });
        Object.defineProperty(Array.prototype, 'toJSON', {
            configurable:true, value() {calls++; throw Error('author array JSON hook');}
        });
        // Even if an author creates this spelling, it is not the private hook.
        globalThis.__cloneCanvasBindings = new Proxy({}, {get() {calls++; throw Error('author clone hook');}});
        const canvas = new OffscreenCanvas(3, 4);
        const cloned = structuredClone({canvas, list:[1,2,3]}, {transfer:[canvas]});
        assert(cloned.canvas.width === 3 && cloned.list[2] === 3, 'isolated Canvas clone');
        assert(calls === 0, 'internal wire serialization has no author JSON callbacks');
    } finally {
        restore(Object.prototype, oldObject);restore(Array.prototype, oldArray);
        if (oldHook === undefined) delete globalThis.__cloneCanvasBindings;
        else globalThis.__cloneCanvasBindings = oldHook;
    }
    const own = Object.create(null);
    Object.defineProperty(own, '__proto__', {enumerable:true, value:{polluted:true}});
    const cloned = structuredClone(own);
    assert(Object.getOwnPropertyDescriptor(cloned, '__proto__').value.polluted === true,
        'clone preserves the own __proto__ data property');
    assert(Object.getPrototypeOf(cloned) === null, 'data property does not replace the prototype');
    const ordinary = JSON.parse('{"__proto__":{"polluted":true},"value":1}');
    const ordinaryClone = structuredClone(ordinary);
    assert(Object.getPrototypeOf(ordinaryClone) === Object.prototype &&
        Object.prototype.hasOwnProperty.call(ordinaryClone, '__proto__'),
        'ordinary object clone defines properties rather than invoking setters');
    return {passed:true};
}
