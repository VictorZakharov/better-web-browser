function testCanvasImageUsability(makeCanvas) {
    const assert = (value, label) => { if (!value) throw Error(label); };
    const throws = (name, operation, label) => {
        let error;
        try { operation(); } catch (value) { error = value; }
        assert(error?.name === name, label + ': ' + error?.name);
    };
    const target = makeCanvas(32, 32), context = target.getContext('2d');
    const source = makeCanvas(4, 4), sourceContext = source.getContext('2d');
    sourceContext.fillStyle = 'red'; sourceContext.fillRect(0, 0, 4, 4);
    // Platform branding is independent of the author's prototype chain.
    Object.setPrototypeOf(source, null);
    let conversions = 0;
    const pattern = context.createPattern(source, { toString() { conversions++; return 'repeat'; } });
    assert(pattern && conversions === 1, 'private Canvas source brand and one DOMString conversion');
    context.fillStyle = pattern; context.fillRect(0, 0, 32, 32);
    assert(context.getImageData(2, 2, 1, 1).data[0] === 255, 'branded pattern owns real source pixels');
    sourceContext.fillStyle = 'blue'; sourceContext.fillRect(0, 0, 4, 4);
    context.fillRect(0, 0, 32, 32);
    assert(context.getImageData(2, 2, 1, 1).data[0] === 255, 'pattern snapshots source at creation');
    throws('TypeError', () => context.createPattern({}, { toString() { throw Error('later conversion'); } }),
        'source brand before repetition conversion');
    throws('TypeError', () => context.createPattern(source, Symbol()), 'DOMString rejects Symbol');
    throws('SyntaxError', () => context.createPattern(source, 'invalid'), 'valid source invalid repetition');
    throws('InvalidStateError', () => context.createPattern(makeCanvas(0, 4), 'invalid'),
        'zero Canvas usability before repetition validation');
    let closed;
    const bitmapSource = new OffscreenCanvas(2, 2);
    bitmapSource.getContext('2d'); closed = bitmapSource.transferToImageBitmap(); closed.close();
    throws('InvalidStateError', () => context.createPattern(closed, 'invalid'), 'closed bitmap usability');
    context.drawImage(closed, NaN, 0); // Nonfinite numeric arguments precede usability.
    throws('InvalidStateError', () => context.drawImage(closed, 0, 0), 'closed bitmap drawing');
    return { passed: true };
}

function testHtmlImageUsability() {
    const assert = (value, label) => { if (!value) throw Error(label); };
    const canvas = document.createElement('canvas'); canvas.width = canvas.height = 16;
    const context = canvas.getContext('2d');
    context.fillStyle = 'red'; context.fillRect(0, 0, 16, 16);
    const image = document.createElement('img');
    for (const name of ['complete', 'naturalWidth', 'naturalHeight', 'src', 'srcset', 'getAttribute', 'hasAttribute'])
        Object.defineProperty(image, name, { get() { throw Error('author image accessor ' + name); } });
    context.globalCompositeOperation = 'copy';
    context.shadowColor = 'blue'; context.shadowOffsetX = 2;
    context.filter = 'blur(1px)';
    context.drawImage(image, 0, 0);
    assert(context.getImageData(0, 0, 1, 1).data[0] === 255, 'unavailable image does not clear copy backdrop');
    let converted = 0;
    assert(context.createPattern(image, { toString() { converted++; return 'invalid'; } }) === null,
        'unavailable image returns null before invalid repetition');
    assert(converted === 1, 'incomplete image still converts repetition');
    let error;
    try { context.createPattern(image, Symbol()); } catch (value) { error = value; }
    assert(error?.name === 'TypeError', 'conversion error before incomplete usability');
    Object.setPrototypeOf(image, null);
    context.drawImage(image, 0, 0);
    assert(context.createPattern(image, 'repeat') === null, 'prototype changes do not remove image brand');
    return { passed: true };
}
