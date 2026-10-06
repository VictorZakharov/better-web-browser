function testCanvasRectangleBindings(makeCanvas) {
    const assert = (value, label) => { if (!value) throw Error(label); };
    const throws = (operation, label) => {
        let error;
        try { operation(); } catch (value) { error = value; }
        assert(error?.name === 'TypeError', label + ': ' + error?.name);
    };
    const canvas = makeCanvas(32, 32), context = canvas.getContext('2d');
    for (const name of ['clearRect', 'fillRect', 'strokeRect']) {
        const method = context[name];
        assert(method.length === 4, name + ' IDL arity');
        for (let count = 0; count < 4; count++)
            throws(() => method.apply(context, Array(count).fill(1)), name + ' required arity ' + count);
        let reads = 0;
        const argument = { valueOf() { reads++; return 1; } };
        throws(() => method.call({}, argument, 1, 1, 1), name + ' receiver before conversion');
        assert(reads === 0, name + ' invalid receiver does not read arguments');
        for (let index = 0; index < 4; index++) for (const invalid of [1n, Symbol()]) {
            const values = [1, 1, 1, 1]; values[index] = invalid;
            throws(() => method.apply(context, values), name + ' numeric argument ' + index);
        }
        const order = [];
        const values = [1, 1, 3, 3].map((value, index) => ({ valueOf() { order.push(index); return value; } }));
        method.apply(context, values);
        assert(order.join(',') === '0,1,2,3', name + ' converts once left to right');
        method.call(context, NaN, 1, 3, argument);
        assert(reads === 1, name + ' converts all arguments before nonfinite no-op');
        method.call(context, 1, 1, 3, 3, Symbol('ignored extra'));
    }
    // CTM changes from conversion take effect before the drawing algorithm.
    context.clearRect(0, 0, 32, 32); context.fillStyle = 'red';
    context.fillRect({ valueOf() { context.translate(10, 0); return 0; } }, 0, 4, 4);
    assert(context.getImageData(1, 1, 1, 1).data[3] === 0, 'rectangle does not capture old CTM');
    assert(context.getImageData(11, 1, 1, 1).data[0] === 255, 'rectangle uses CTM after conversion');
    context.resetTransform(); context.globalCompositeOperation = 'copy';
    context.fillRect(NaN, 0, 4, 4);
    assert(context.getImageData(11, 1, 1, 1).data[0] === 255, 'nonfinite rectangle does not clear copy backdrop');
    context.beginPath(); context.rect(1, 1, 4, 4);
    for (const name of ['fillRect', 'strokeRect', 'clearRect']) context[name](10, 10, 2, 2);
    assert(context.isPointInPath(2, 2) && !context.isPointInPath(11, 11), 'rectangle painting leaves path unchanged');
    return { passed: true };
}
