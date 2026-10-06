function testImageDataConstruction() {
    const assert = (condition, name) => { if (!condition) throw Error(name); };
    const throws = (operation, name, label) => {
        let error;
        try { operation(); } catch (caught) { error = caught; }
        assert(error && error.name === name, label);
    };
    throws(() => new ImageData(), 'TypeError', 'constructor required arity');
    throws(() => new ImageData(1), 'TypeError', 'constructor requires height');
    const image = new ImageData(4294967297.9, 4294967298.9);
    assert(image.width === 1 && image.height === 2 && image.data.length === 8, 'unsigned-long wrap/truncate');
    assert(image.colorSpace === 'srgb' && image.pixelFormat === 'rgba-unorm8', 'actual storage format');
    for (const value of [NaN, Infinity, -Infinity, 0, 4294967296])
        throws(() => new ImageData(value, 1), 'IndexSizeError', 'zero-converted width');
    for (const value of [1n, Symbol()]) {
        throws(() => new ImageData(value,1), 'TypeError', 'width ToNumber');
        throws(() => new ImageData(1,value), 'TypeError', 'height ToNumber');
    }
    const order = [];
    const number = (label, value) => ({valueOf() { order.push(label); return value; }});
    const options = {
        get colorSpace() { order.push('colorSpace'); return {toString() {order.push('cs.convert');return 'srgb';}}; },
        get pixelFormat() { order.push('pixelFormat'); return {toString() {order.push('pf.convert');return 'rgba-unorm8';}}; }
    };
    new ImageData(number('width',1),number('height',1),options);
    assert(order.join(',') === 'width,height,colorSpace,cs.convert,pixelFormat,pf.convert', 'constructor conversion order');
    throws(() => new ImageData(0,0,{colorSpace:'invalid'}), 'TypeError', 'settings conversion precedes zero-size algorithm');
    for (const value of [0, '', true, 1n, Symbol()])
        throws(() => new ImageData(1,1,value), 'TypeError', 'settings must be dictionary');
    const buffer = new ArrayBuffer(32), data = new Uint8ClampedArray(buffer,8,16);
    data.set([12,34,56,78]);
    const borrowed = new ImageData(data,2);
    assert(borrowed.data === data && borrowed.width === 2 && borrowed.height === 2, 'input view identity and inferred height');
    data[0] = 99;
    assert(borrowed.data[0] === 99, 'borrowed view remains live');
    Object.defineProperties(data, {
        length:{get() {throw Error('author length');}}, byteLength:{get() {throw Error('author byteLength');}},
        buffer:{get() {throw Error('author buffer');}}, byteOffset:{get() {throw Error('author offset');}}
    });
    const protectedView = new ImageData(data,2,2);
    assert(protectedView.data === data && protectedView.height === 2, 'constructor uses intrinsic metadata');
    const bare = new Uint8ClampedArray([1,2,3,4]);
    Object.setPrototypeOf(bare,null);
    assert(new ImageData(bare,1).data === bare, 'typed-array brand independent of prototype');
    for (const size of [0,1,2,3,5,6,7])
        throws(() => new ImageData(new Uint8ClampedArray(size),1), 'InvalidStateError', 'non-integral pixel storage');
    throws(() => new ImageData(new Uint8ClampedArray(8),3), 'IndexSizeError', 'width mismatch');
    throws(() => new ImageData(new Uint8ClampedArray(8),1,1), 'IndexSizeError', 'height mismatch');
    throws(() => new ImageData(new Uint8ClampedArray(4),1,{colorSpace:'srgb'}), 'IndexSizeError',
        'third array overload argument is unsigned-long height, not settings');
    const resizable = new Uint8ClampedArray(new ArrayBuffer(4,{maxByteLength:8}));
    throws(() => new ImageData(resizable,1), 'TypeError', 'resizable buffer source rejected');
    const detached = new Uint8ClampedArray(4);
    structuredClone(detached.buffer,{transfer:[detached.buffer]});
    throws(() => new ImageData(detached,1), 'InvalidStateError', 'detached input has no pixels');
    for (const key of ['width','height','data','colorSpace','pixelFormat']) {
        const descriptor = Object.getOwnPropertyDescriptor(ImageData.prototype,key);
        assert(descriptor.enumerable && descriptor.configurable && !descriptor.set, 'readonly IDL property ' + key);
        throws(() => descriptor.get.call({}), 'TypeError', 'private getter brand ' + key);
    }
    throws(() => new ImageData(1,1,{pixelFormat:'rgba-float16'}), 'NotSupportedError', 'unsupported storage not advertised');
    throws(() => new ImageData(new Uint8ClampedArray(8),1,1,{pixelFormat:'rgba-float16'}),
        'InvalidStateError', 'array/format mismatch before provider admission');
}

function testCanvasImageDataBindings(make) {
    const assert = (condition, name) => { if (!condition) throw Error(name); };
    const throws = (operation, name, label) => {
        let error;
        try { operation(); } catch (caught) { error = caught; }
        assert(error && error.name === name, label);
    };
    const context = make(8,8).getContext('2d');
    throws(() => context.createImageData(), 'TypeError', 'create required arity');
    throws(() => context.createImageData(1), 'TypeError', 'one-argument overload requires ImageData');
    const source = new ImageData(2,3);
    source.data[0] = 200;
    for (const key of ['data','width','height','colorSpace','pixelFormat'])
        Object.defineProperty(source,key,{get() {throw Error('source expando ' + key);}});
    const copy = context.createImageData(source);
    assert(copy.width === 2 && copy.height === 3 && copy.data[0] === 0, 'copy uses private dimensions and blank new storage');
    const negative = context.createImageData(-2.9,-3.9);
    assert(negative.width === 2 && negative.height === 3, 'create EnforceRange long then absolute size');
    for (const value of [NaN,Infinity,-Infinity,2147483648,-2147483649,1n,Symbol()])
        throws(() => context.createImageData(value,1), 'TypeError', 'create EnforceRange rejects invalid width');
    const order = [];
    const value = (label, result) => ({valueOf() {order.push(label); return result;}});
    throws(() => context.getImageData(value('x',0),value('y',0),value('w',0),value('h',0),{
        get colorSpace() {order.push('cs');return 'invalid';}
    }), 'TypeError', 'get settings conversion before zero-size algorithm');
    assert(order.join(',') === 'x,y,w,h,cs', 'readback conversion order');
    for (let count=0;count<4;count++)
        throws(() => context.getImageData(...Array(count).fill(1)), 'TypeError', 'readback arity');
    const put = new ImageData(new Uint8ClampedArray([20,40,60,255]),1);
    for (let count=0;count<3;count++)
        throws(() => context.putImageData(...[put,0,0].slice(0,count)), 'TypeError', 'raw write arity');
    context.putImageData(put,0,0);
    assert(context.getImageData(0,0,1,1).data.join(',') === '20,40,60,255', 'normal read/write retained');
}

function testImageDataStructuredClone() {
    const assert = (condition, name) => { if (!condition) throw Error(name); };
    const storage = new ArrayBuffer(24), data = new Uint8ClampedArray(storage,8,8);
    data.set([12,34,56,78,90,100,110,120]);
    const image = new ImageData(data,2,1);
    const copy = structuredClone({image, data, storage, again:image});
    assert(copy.image instanceof ImageData && copy.again === copy.image, 'clone ImageData identity');
    assert(copy.image.data === copy.data && copy.data.buffer === copy.storage,
        'ImageData sub-serialization retains graph view/buffer identity');
    assert(copy.data.byteOffset === 8 && copy.data.length === 8 && copy.storage !== storage,
        'clone preserves source view metadata with independent storage');
    copy.data[0] = 200;
    assert(copy.image.data[0] === 200 && data[0] === 12, 'clone aliases only its own storage');
    const reverse = structuredClone({data,storage,image});
    assert(reverse.image.data === reverse.data && reverse.data.buffer === reverse.storage, 'references resolved in either graph order');
    const saved = ImageData;
    for (const key of ['data','width','height','colorSpace','pixelFormat'])
        Object.defineProperty(image,key,{get() {throw Error('clone author getter ' + key);}});
    globalThis.ImageData = function() {throw Error('author constructor');};
    globalThis.__cloneImageDataBindings = {has() {throw Error('author clone hook');}};
    let poisoned;
    try { poisoned = structuredClone(image); }
    finally { globalThis.ImageData = saved; delete globalThis.__cloneImageDataBindings; }
    assert(poisoned instanceof saved && poisoned.width === 2 && poisoned.data[0] === 12,
        'clone uses captured platform slots and receiver constructor');
    const transfer = structuredClone({image,data,storage}, {transfer:[storage]});
    assert(data.byteLength === 0 && transfer.image.data === transfer.data && transfer.data.buffer === transfer.storage,
        'transferred backing buffer remains shared with cloned ImageData');
    let error;
    try { structuredClone(image); } catch (caught) {error=caught;}
    assert(error && error.name === 'DataCloneError', 'detached ImageData cannot be cloned');
}
