// Shared Window/Worker coverage of the HTML Canvas drawing-state IDL.
function testCanvasDrawingStateConversions(make) {
    const context = make(32, 32).getContext('2d');
    const assert = (condition, name) => { if (!condition) throw Error(name); };
    const throws = (operation, name, label) => {
        let error;
        try { operation(); } catch (caught) { error = caught; }
        assert(error && error.name === name, label);
    };
    const numeric = [
        ['globalAlpha', 0.4, [NaN, Infinity, -Infinity, -0.1, 1.1]],
        ['shadowBlur', 2.5, [NaN, Infinity, -Infinity, -1]],
        ['shadowOffsetX', -3.5, [NaN, Infinity, -Infinity]],
        ['shadowOffsetY', 4.5, [NaN, Infinity, -Infinity]],
        ['lineWidth', 3, [NaN, Infinity, -Infinity, 0, -1]],
        ['miterLimit', 5, [NaN, Infinity, -Infinity, 0, -1]],
        ['lineDashOffset', -6, [NaN, Infinity, -Infinity]]
    ];
    for (const [name, accepted, ignored] of numeric) {
        let conversions = 0;
        context[name] = {[Symbol.toPrimitive](hint) {
            assert(hint === 'number', name + ' numeric hint');
            conversions++;
            return accepted;
        }};
        assert(conversions === 1 && context[name] === accepted, name + ' numeric conversion');
        for (const invalid of ignored) {
            context[name] = invalid;
            assert(context[name] === accepted, name + ' ignores invalid number');
        }
        throws(() => { context[name] = 1n; }, 'TypeError', name + ' rejects BigInt');
        throws(() => { context[name] = Symbol(); }, 'TypeError', name + ' rejects Symbol');
        const sentinel = Error(name);
        let actual;
        try { context[name] = {valueOf() { throw sentinel; }}; } catch (error) { actual = error; }
        assert(actual === sentinel && context[name] === accepted, name + ' propagates conversion error');
    }
    const strings = [
        ['globalCompositeOperation', 'multiply'], ['fillStyle', '#ff0000'],
        ['strokeStyle', '#ff0000'], ['shadowColor', '#ff0000'],
        ['filter', 'blur(1px)'], ['font', '12px sans-serif'],
        ['textAlign', 'center'], ['textBaseline', 'middle'],
        ['direction', 'rtl'], ['imageSmoothingQuality', 'high'],
        ['lineCap', 'round'], ['lineJoin', 'bevel']
    ];
    for (const [name, accepted] of strings) {
        let conversions = 0;
        context[name] = {[Symbol.toPrimitive](hint) {
            assert(hint === 'string', name + ' string hint');
            conversions++;
            return accepted;
        }};
        assert(conversions === 1 && context[name] === accepted, name + ' string conversion');
        throws(() => { context[name] = Symbol(); }, 'TypeError', name + ' rejects Symbol');
        const previous = context[name];
        context[name] = 'not-a-valid-value';
        assert(context[name] === previous, name + ' preserves state on invalid syntax');
        const sentinel = Error(name);
        let actual;
        try { context[name] = {toString() { throw sentinel; }}; } catch (error) { actual = error; }
        assert(actual === sentinel && context[name] === previous, name + ' propagates string error');
    }
    context.imageSmoothingEnabled = {valueOf() { throw Error('boolean must not coerce'); }};
    assert(context.imageSmoothingEnabled === true, 'boolean object conversion');
    context.imageSmoothingEnabled = 0n;
    assert(context.imageSmoothingEnabled === false, 'boolean accepts BigInt truthiness');
    context.imageSmoothingEnabled = Symbol();
    assert(context.imageSmoothingEnabled === true, 'boolean accepts Symbol truthiness');
    const gradient = context.createLinearGradient(0, 0, 10, 0);
    Object.defineProperty(gradient, Symbol.toPrimitive, {value() { throw Error('gradient coerced'); }});
    context.fillStyle = gradient;
    assert(context.fillStyle === gradient, 'gradient union uses platform brand');
    context.save();
    context.globalAlpha = 1;
    context.shadowBlur = 0;
    context.direction = 'ltr';
    context.restore();
    assert(context.globalAlpha === 0.4 && context.shadowBlur === 2.5 && context.direction === 'rtl',
        'converted drawing state survives save/restore');
}

function testCanvasTextArgumentConversions(make) {
    const context = make(32, 32).getContext('2d');
    const assert = (condition, name) => { if (!condition) throw Error(name); };
    const throws = (operation, label) => {
        let error;
        try { operation(); } catch (caught) { error = caught; }
        assert(error && error.name === 'TypeError', label);
    };
    for (const name of ['fillText', 'strokeText']) {
        for (let count = 0; count < 3; count++)
            throws(() => context[name](...Array(count).fill('x')), name + ' required arity');
        const order = [];
        const text = {[Symbol.toPrimitive](hint) {
            assert(hint === 'string', name + ' text hint'); order.push('text'); return 'x';
        }};
        const number = (label, value) => ({[Symbol.toPrimitive](hint) {
            assert(hint === 'number', name + ' ' + label + ' hint'); order.push(label); return value;
        }});
        context[name](text, number('x', NaN), number('y', 0), number('maxWidth', -1),
            {valueOf() { throw Error('surplus argument converted'); }});
        assert(order.join(',') === 'text,x,y,maxWidth', name + ' converts every argument before no-op');
        for (const index of [0, 1, 2, 3]) {
            const args = ['x', 0, 0, 20];
            args[index] = Symbol();
            throws(() => context[name](...args), name + ' rejects Symbol at ' + index);
            if (index > 0) {
                args[index] = 1n;
                throws(() => context[name](...args), name + ' rejects BigInt at ' + index);
            }
        }
        const sentinel = Error('text conversion');
        let caught;
        try { context[name]({toString() { throw sentinel; }}, NaN, 0); }
        catch (error) { caught = error; }
        assert(caught === sentinel, name + ' text conversion precedes nonfinite check');
        context.reset();
        context.fillStyle = '#ff0000';
        context.fillRect(0, 0, 32, 32);
        context.globalCompositeOperation = 'copy';
        for (const args of [['', 0, 0], ['x', NaN, 0], ['x', 0, Infinity], ['x', 0, 0, 0]]) {
            context[name](...args);
            const pixel = context.getImageData(0, 0, 1, 1).data;
            assert(pixel.join(',') === '255,0,0,255', name + ' no-op must not composite empty layer');
        }
    }
    throws(() => context.measureText(), 'measureText required arity');
    throws(() => context.measureText(Symbol()), 'measureText DOMString');
    let conversions = 0;
    const metrics = context.measureText({toString() { conversions++; return 'A\tB'; }});
    assert(conversions === 1 && metrics.width === context.measureText('A B').width,
        'measureText converts once and replaces ASCII whitespace');
}
