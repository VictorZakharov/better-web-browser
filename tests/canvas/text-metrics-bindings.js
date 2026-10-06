function testCanvasTextMetrics(make) {
    const assert = (condition, name) => { if (!condition) throw Error(name); };
    const near = (actual, expected, name) => assert(Math.abs(actual-expected)<1e-7, name);
    const context = make(160,100).getContext('2d');
    context.font = '24px Arial';
    const metric = context.measureText('Ag');
    assert(metric instanceof TextMetrics && metric.width > 0, 'real shaped advance');
    let error;
    try { new TextMetrics(); } catch (caught) {error=caught;}
    assert(error && error.name === 'TypeError', 'TextMetrics illegal constructor');
    const names = ['width','actualBoundingBoxLeft','actualBoundingBoxRight',
        'actualBoundingBoxAscent','actualBoundingBoxDescent','fontBoundingBoxAscent',
        'fontBoundingBoxDescent','emHeightAscent','emHeightDescent','hangingBaseline',
        'alphabeticBaseline','ideographicBaseline'];
    for (const name of names) {
        const descriptor = Object.getOwnPropertyDescriptor(TextMetrics.prototype,name);
        assert(descriptor.enumerable && descriptor.configurable && !descriptor.set,
            'readonly metrics descriptor ' + name);
        let caught;
        try { descriptor.get.call({}); } catch (error) {caught=error;}
        assert(caught && caught.name === 'TypeError', 'private metrics receiver ' + name);
        assert(Number.isFinite(descriptor.get.call(metric)), 'finite metric ' + name);
        assert(!Object.hasOwn(metric,name), 'metrics have prototype IDL getters ' + name);
    }
    const width = metric.width;
    Reflect.set(metric,'width',1000);
    assert(metric.width === width, 'readonly metrics cannot be assigned');
    context.translate(40,30); context.scale(2,3); context.rotate(0.1);
    const transformed = context.measureText('Ag');
    for (const name of names) near(transformed[name],metric[name], 'metrics ignore CTM ' + name);
    context.textAlign = 'center';
    const center = context.measureText('Ag');
    near(center.width,metric.width,'center advance unchanged');
    near(center.actualBoundingBoxLeft,metric.actualBoundingBoxLeft+width/2,'center left alignment');
    near(center.actualBoundingBoxRight,metric.actualBoundingBoxRight-width/2,'center right alignment');
    context.textAlign = 'right';
    const right = context.measureText('Ag');
    near(right.actualBoundingBoxLeft,metric.actualBoundingBoxLeft+width,'right left alignment');
    near(right.actualBoundingBoxRight,metric.actualBoundingBoxRight-width,'right right alignment');
    context.direction = 'rtl'; context.textAlign = 'start';
    const rtl = context.measureText('Ag');
    near(rtl.actualBoundingBoxLeft,right.actualBoundingBoxLeft,'RTL start alignment');
    context.direction = 'ltr'; context.textAlign = 'left';
    const alphabetic = context.measureText(',');
    for (const baseline of ['top','hanging','middle','ideographic','bottom']) {
        context.textBaseline = baseline;
        const moved = context.measureText(',');
        const delta = moved.alphabeticBaseline - alphabetic.alphabeticBaseline;
        near(moved.width,alphabetic.width,'baseline advance ' + baseline);
        near(moved.actualBoundingBoxAscent,alphabetic.actualBoundingBoxAscent+delta,'baseline ascent ' + baseline);
        near(moved.actualBoundingBoxDescent,alphabetic.actualBoundingBoxDescent-delta,'baseline descent ' + baseline);
        near(moved.fontBoundingBoxAscent,alphabetic.fontBoundingBoxAscent+delta,'font ascent ' + baseline);
        near(moved.fontBoundingBoxDescent,alphabetic.fontBoundingBoxDescent-delta,'font descent ' + baseline);
        near(moved.hangingBaseline,alphabetic.hangingBaseline+delta,'hanging distance ' + baseline);
        near(moved.ideographicBaseline,alphabetic.ideographicBaseline+delta,'ideographic distance ' + baseline);
    }
    context.textBaseline = 'top';
    assert(context.measureText(',').actualBoundingBoxAscent < 0, 'ink below selected baseline has negative ascent');
    context.textAlign = 'center';
    for (const text of ['', '   ']) {
        const empty = context.measureText(text);
        for (const name of ['actualBoundingBoxLeft','actualBoundingBoxRight',
            'actualBoundingBoxAscent','actualBoundingBoxDescent'])
            assert(empty[name] === 0, 'no ink bounds ' + name);
    }
    context.reset();
    assert(metric.width === width && metric.actualBoundingBoxAscent > 0, 'metrics remain immutable snapshots after reset');
}
