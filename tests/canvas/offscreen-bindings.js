// Shared Window/Worker contract. Large logical dimensions must not allocate
// beyond the browser's bounded bitmap budget or silently saturate to uint32.
function testOffscreenBindings() {
    const assert = (value, label) => { if (!value) throw new Error(label); };
    const throws = (callback, name, label) => {
        let actual = '';
        try { callback(); } catch (error) { actual = error.name; }
        assert(actual === name, `${label}: ${actual}`);
    };
    const valid = [[0, 0], [-0.75, 0], [2.9, 2], ['3.9', 3], [null, 0],
        [true, 1], [4294967296, 4294967296], [9007199254740991, 9007199254740991]];
    for (const [input, expected] of valid) {
        const canvas = new OffscreenCanvas(input, 0);
        assert(canvas.width === expected && !Object.is(canvas.width, -0), 'constructor conversion');
        canvas.height = input;
        assert(canvas.height === expected, 'setter conversion');
    }
    for (const value of [undefined, NaN, Infinity, -Infinity, -1, -1.9,
        9007199254740992, 1n, Symbol('dimension')]) {
        throws(() => new OffscreenCanvas(value, 1), 'TypeError', 'invalid constructor');
        const canvas = new OffscreenCanvas(2, 3);
        throws(() => { canvas.width = value; }, 'TypeError', 'invalid setter');
        assert(canvas.width === 2 && canvas.height === 3, 'failed setter is atomic');
    }
    const log = [];
    const width = {valueOf() { log.push('width'); return 2; }};
    const height = {valueOf() { log.push('height'); return 3; }};
    const canvas = new OffscreenCanvas(width, height);
    assert(log.join(',') === 'width,height', 'constructor conversion order');
    const widthSetter = Object.getOwnPropertyDescriptor(OffscreenCanvas.prototype, 'width').set;
    throws(() => widthSetter.call({}, width), 'TypeError', 'setter receiver');
    assert(log.length === 2, 'receiver before argument conversion');
    throws(() => canvas.getContext(), 'TypeError', 'required context argument');
    for (const identifier of ['2D', '', 'experimental-webgl', undefined, null, Symbol('context')])
        throws(() => canvas.getContext(identifier), 'TypeError', 'context enum');
    const context = canvas.getContext('2d');
    context.fillStyle = 'red'; context.fillRect(0, 0, 1, 1);
    canvas.width = 2;
    assert(context.fillStyle === '#000000' && context.getImageData(0, 0, 1, 1).data[3] === 0,
        'redundant dimension setter resets context and pixels');
    for (const mode of ['2d', 'bitmaprenderer']) {
        const active = new OffscreenCanvas(1, 1), original = active.getContext(mode);
        throws(() => structuredClone(active, {transfer: [active]}), 'InvalidStateError', 'active transfer');
        assert(active.width === 1 && active.getContext(mode) === original, 'rejected transfer keeps owner');
    }
    const source = new OffscreenCanvas(3, 2);
    const moved = structuredClone(source, {transfer: [source]});
    assert(source.width === 0 && source.height === 0 && moved.width === 3 && moved.height === 2,
        'uninitialized transfer dimensions');
    assert(moved.getContext('2d').getImageData(0, 0, 1, 1).data[3] === 0, 'transfer starts transparent');
    let converted = 0;
    const identifier = {toString() { converted++; return '2d'; }};
    throws(() => source.getContext(identifier), 'InvalidStateError', 'detached context');
    assert(converted === 1, 'context conversion precedes detached algorithm');
    throws(() => { source.width = {valueOf() { converted++; return 1; }}; },
        'InvalidStateError', 'detached setter');
    assert(converted === 2, 'dimension conversion precedes detached algorithm');
    throws(() => { source.height = 1n; }, 'TypeError', 'conversion error precedes detach');
    throws(() => source.transferToImageBitmap(), 'InvalidStateError', 'detached bitmap');
    return {passed: true};
}
