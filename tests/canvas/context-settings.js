function testCanvasContextSettings(make) {
    const assert = (condition, name) => { if (!condition) throw Error(name); };
    const throws = (operation, label) => {
        let error;
        try { operation(); } catch (caught) { error = caught; }
        assert(error && error.name === 'TypeError', label);
    };
    const defaults = make(8, 8).getContext('2d').getContextAttributes();
    assert(JSON.stringify(defaults) === JSON.stringify({alpha:true, colorSpace:'srgb',
        colorType:'unorm8', desynchronized:false, willReadFrequently:false}), 'actual default settings');
    const order = [];
    const options = {};
    for (const [key, value] of [['alpha', false], ['colorSpace', 'srgb'], ['colorType', 'unorm8'],
        ['desynchronized', true], ['willReadFrequently', true]]) {
        Object.defineProperty(options, key, {get() {
            order.push(key);
            if (key === 'colorSpace' || key === 'colorType') return {toString() {
                order.push(key + '.convert'); return value;
            }};
            return value;
        }});
    }
    const canvas = make(8, 8), context = canvas.getContext('2d', options);
    assert(order.join(',') === 'alpha,colorSpace,colorSpace.convert,colorType,colorType.convert,' +
        'desynchronized,willReadFrequently', 'dictionary reads and conversion are interleaved');
    const actual = context.getContextAttributes();
    assert(!actual.alpha && actual.willReadFrequently && !actual.desynchronized, 'supported settings retained');
    actual.alpha = true;
    actual.colorType = 'float16';
    assert(!context.getContextAttributes().alpha && context.getContextAttributes().colorType === 'unorm8',
        'attributes returns a fresh result');
    const repeated = canvas.getContext('2d', new Proxy({}, {get() { throw Error('existing options read'); }}));
    assert(repeated === context, 'existing context does not convert options');
    context.reset();
    canvas.width = canvas.width;
    assert(!context.getContextAttributes().alpha && context.getContextAttributes().willReadFrequently,
        'settings survive reset and resize');
    for (const value of [0, false, '', Symbol(), 1n])
        throws(() => make(8, 8).getContext('2d', value), 'non-dictionary options rejected');
    for (const value of [null, undefined])
        assert(make(8, 8).getContext('2d', value).getContextAttributes().alpha, 'nullish default dictionary');
    for (const key of ['colorSpace', 'colorType']) {
        for (const value of ['invalid', Symbol(), null]) {
            const unused = make(8, 8);
            throws(() => unused.getContext('2d', {[key]:value}), key + ' enum rejects invalid value');
            assert(unused.getContext('2d') !== null, 'failed conversion does not bind context mode');
        }
    }
    const sentinel = Error('settings getter');
    let caught;
    const failing = make(8, 8);
    try { failing.getContext('2d', {get alpha() { throw sentinel; }}); }
    catch (error) { caught = error; }
    assert(caught === sentinel && failing.getContext('2d') !== null, 'dictionary failure is atomic');
    // This provider intentionally declines unsupported actual bitmap formats.
    // Do not treat a returned sRGB/unorm8 context as display-p3 or float16 support.
    for (const options of [{colorSpace:'display-p3'}, {colorType:'float16'}]) {
        const unsupported = make(8, 8);
        assert(unsupported.getContext('2d', options) === null, 'unsupported backend format declines creation');
        assert(unsupported.getContext('2d') !== null, 'format decline leaves mode available');
    }
}

function testOpaqueCanvasPixels(make) {
    const assert = (condition, name) => { if (!condition) throw Error(name); };
    const canvas = make(32, 32), context = canvas.getContext('2d', {alpha:false});
    const pixel = (x=0, y=0) => context.getImageData(x,y,1,1).data.join(',');
    assert(pixel() === '0,0,0,255', 'opaque canvas starts black');
    assert(pixel(-1,0) === '0,0,0,0', 'outside bitmap remains transparent');
    context.fillStyle = 'rgba(255, 0, 0, 0.5)';
    context.fillRect(0,0,32,32);
    assert(pixel() === '128,0,0,255', 'source alpha remains significant on opaque output');
    context.globalCompositeOperation = 'copy';
    context.fillRect(2,2,4,4);
    assert(pixel(3,3) === '128,0,0,255' && pixel() === '0,0,0,255',
        'copy retains premultiplied RGB and clears outside source to opaque black');
    context.clearRect(2,2,4,4);
    assert(pixel(3,3) === '0,0,0,255', 'clear preserves opaque alpha');
    for (const alpha of [0,1,64,128,255]) {
        const data = new ImageData(new Uint8ClampedArray([170,90,40,alpha]), 1, 1);
        context.putImageData(data,3,3);
        assert(pixel(3,3) === '170,90,40,255', 'putImageData ignores source alpha, including hidden RGB');
        assert(data.data[3] === alpha, 'putImageData does not mutate source storage');
    }
    context.save();
    context.beginPath(); context.rect(2,2,4,4); context.clip();
    context.clearRect(0,0,32,32);
    assert(pixel(3,3) === '0,0,0,255', 'clipped clear is opaque');
    context.restore();
    context.reset();
    assert(pixel() === '0,0,0,255', 'reset clears to opaque black');
    canvas.width = canvas.width;
    assert(pixel() === '0,0,0,255' && !context.getContextAttributes().alpha, 'resize keeps opaque settings');
    context.translate(1.2, 1.4); context.rotate(0.3);
    context.clearRect(0,0,4,4);
    const bytes = context.getImageData(0,0,32,32).data;
    for (let offset=3; offset<bytes.length; offset+=4) assert(bytes[offset] === 255, 'transformed clear alpha');
    if (typeof canvas.transferToImageBitmap === 'function') {
        context.reset();
        context.fillStyle = 'rgba(255, 0, 0, 0.5)'; context.fillRect(0,0,32,32);
        const bitmap = canvas.transferToImageBitmap();
        assert(pixel() === '0,0,0,255', 'transfer replaces output with opaque black');
        const target = make(32,32).getContext('2d');
        target.drawImage(bitmap,0,0);
        assert(target.getImageData(0,0,1,1).data.join(',') === '128,0,0,255', 'transfer owns opaque snapshot');
        bitmap.close();
    }
}

function testOpaqueCanvasNativeFallbackParity(make) {
    const operators = ['source-over','source-in','source-out','source-atop','destination-over',
        'destination-in','destination-out','destination-atop','xor','copy','lighter','multiply',
        'screen','overlay','darken','lighten','color-dodge','color-burn','hard-light','soft-light',
        'difference','exclusion','hue','saturation','color','luminosity'];
    const render = (size, operator, shadow, clip) => {
        const context = make(size,size).getContext('2d',{alpha:false});
        context.fillStyle = '#2468ac'; context.fillRect(0,0,size,size);
        if (clip) { context.beginPath(); context.rect(1,1,5,5); context.clip(); }
        context.globalCompositeOperation = operator;
        context.globalAlpha = 0.7;
        if (shadow) {
            context.shadowColor = 'rgba(32, 224, 64, 0.6)';
            context.shadowOffsetX = 1; context.shadowOffsetY = 1;
        }
        context.fillStyle = 'rgba(230, 40, 80, 0.5)';
        context.fillRect(2,2,3,3);
        return context.getImageData(0,0,8,8).data;
    };
    for (const operator of operators) for (const shadow of [false,true]) for (const clip of [false,true]) {
        const scalar = render(8,operator,shadow,clip), native = render(32,operator,shadow,clip);
        for (let index=0; index<scalar.length; index++) {
            if (scalar[index] !== native[index])
                throw Error('opaque parity ' + [operator,shadow,clip,index,scalar[index],native[index]].join(','));
            if (index%4 === 3 && native[index] !== 255) throw Error('opaque result alpha ' + operator);
        }
    }
}
