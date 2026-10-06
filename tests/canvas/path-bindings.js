function testCanvasPathBindings() {
    const assert = (condition, label) => {if (!condition) throw Error(label);};
    const throws = (callback, name, label) => {
        let actual = '';
        try {callback();} catch (error) {actual = error.name;}
        assert(actual === name, `${label}: ${actual}`);
    };
    const canvas = new OffscreenCanvas(32,32), context = canvas.getContext('2d');
    const path = new Path2D();
    const operations = [['moveTo',2],['lineTo',2],['rect',4],['quadraticCurveTo',4],
        ['bezierCurveTo',6],['arcTo',5],['arc',5],['ellipse',7],['roundRect',4]];
    for (const owner of [path,context]) for (const [name,count] of operations) {
        const method = owner[name], args = Array(count).fill(1);
        throws(() => method.apply(owner,args.slice(1)), 'TypeError', `${name} required arity`);
        let converted = 0;
        const effect = {valueOf() {converted++;return 1;}};
        throws(() => method.call({},effect,...args.slice(1)), 'TypeError', `${name} brand`);
        assert(converted === 0, `${name} receiver before conversion`);
        for (let index=0;index<count;index++) {
            const invalid = args.slice();invalid[index] = 1n;
            throws(() => method.apply(owner,invalid), 'TypeError', `${name} BigInt argument ${index}`);
            invalid[index] = Symbol('coordinate');
            throws(() => method.apply(owner,invalid), 'TypeError', `${name} Symbol argument ${index}`);
        }
        const log = [];
        const numbers = args.map((value,index) => ({valueOf() {log.push(index);return value;}}));
        method.apply(owner,numbers);
        assert(log.join(',') === args.map((_,index)=>index).join(','), `${name} each conversion once`);
        // NaN short-circuit belongs to the drawing algorithm, not conversion.
        const nan = args.slice();nan[0] = NaN;nan[count-1] = effect;
        method.apply(owner,nan);
        assert(converted === 1, `${name} converts arguments after NaN`);
        method.apply(owner,[...args, ...(name==='arc'||name==='ellipse' ? [false] :
            name==='roundRect' ? [0] : []), Symbol('ignored extra')]);
    }
    const close = Path2D.prototype.closePath;
    throws(() => close.call({}), 'TypeError', 'closePath receiver');
    throws(() => context.beginPath.call({}), 'TypeError', 'beginPath receiver');
    context.resetTransform();context.beginPath();
    context.moveTo({valueOf() {context.translate(10,0);return 0;}},0);
    context.lineTo(4,0);context.lineTo(4,4);context.lineTo(0,4);context.closePath();
    assert(context.isPointInPath(12,2) && !context.isPointInPath(2,2),
        'path CTM is sampled after all coordinate conversions');
    context.resetTransform();
    let radiusConversions = 0;
    path.arc(8,8,{valueOf() {radiusConversions++;return 2;}},0,Math.PI);
    assert(radiusConversions === 1, 'arc does not convert one radius twice');
    const numericPath = new Path2D({toString() {return 'M1 1H8V8H1Z';}});
    assert(context.isPointInPath(numericPath,3,3), 'Path2D constructor converts DOMString union');
    for (const value of [null, false, 3, 1n]) new Path2D(value);
    throws(() => new Path2D(Symbol('path')), 'TypeError', 'Path2D DOMString rejects Symbol');
    Object.setPrototypeOf(numericPath,null);
    const copy = new Path2D(numericPath);
    assert(context.isPointInPath(copy,3,3) && context.isPointInPath(numericPath,3,3),
        'Path2D copy and draw use native brand, not instanceof');
    for (const name of ['fill','clip']) {
        for (const rule of ['bad','EVENODD',null,1n,Symbol('rule')])
            throws(() => context[name](rule), 'TypeError', `${name} enum`);
        throws(() => context[name]('evenodd','nonzero'), 'TypeError', `${name} arity selects Path2D overload`);
        let calls = 0;
        context[name]({toString() {calls++;return 'nonzero';}});
        assert(calls === 1, `${name} rule conversion once`);
        throws(() => context[name].call({}, {toString() {calls++;return 'bad';}}),
            'TypeError', `${name} receiver`);
        assert(calls === 1, `${name} receiver before enum conversion`);
    }
    for (const name of ['isPointInPath','isPointInStroke']) {
        throws(() => context[name](1), 'TypeError', `${name} arity`);
        throws(() => context[name](1n,2), 'TypeError', `${name} numeric conversion`);
        throws(() => context[name](NaN,Symbol('y')), 'TypeError', `${name} conversion before finite check`);
        assert(context[name](NaN,2) === false, `${name} nonfinite result`);
    }
    throws(() => context.isPointInPath(NaN,2,'bad'), 'TypeError', 'hit enum before finite check');
    throws(() => context.isPointInStroke(1,2,3), 'TypeError', 'stroke hit arity selects external path');
    throws(() => context.stroke(null), 'TypeError', 'stroke path brand');
    context.stroke(numericPath);context.fill(numericPath,'evenodd');
    return {passed:true};
}

function testCanvasRadiusBindings() {
    const assert = (condition,label) => {if (!condition) throw Error(label);};
    const throws = (callback,name,label) => {
        let actual='';try {callback();}catch(error){actual=error.name;}
        assert(actual===name,`${label}: ${actual}`);
    };
    const context = new OffscreenCanvas(20,20).getContext('2d');
    const path = new Path2D(), log=[];
    const radius = {};
    for (const key of ['w','x','y','z']) Object.defineProperty(radius,key,{
        get() {log.push(key);return {valueOf() {log.push(key+'-number');return 2;}};}
    });
    path.roundRect(0,0,10,10,radius);
    assert(log.join(',')==='w,w-number,x,x-number,y,y-number,z,z-number',
        'DOMPointInit reads and converts dictionary members lexicographically');
    let lookups=0,iterations=0;
    const sequence = {get [Symbol.iterator]() {
        lookups++;return function*() {iterations++;yield 1;yield {x:2,y:3};};
    }};
    path.roundRect(0,0,10,10,sequence);
    assert(lookups===1 && iterations===1,'radius sequence reads iterator once');
    for (const radii of [new Set([1,2]),new Float32Array([1,2,3]),null,undefined,{x:2,y:3}])
        path.roundRect(0,0,10,10,radii);
    for (const radii of [[],[1,2,3,4,5],-1,{x:-1,y:2}])
        throws(() => path.roundRect(0,0,10,10,radii),'RangeError','radius algorithm rejects size or sign');
    for (const radii of [1n,Symbol('radius'),[1n],{x:1n},{z:Symbol('unused')}])
        throws(() => path.roundRect(0,0,10,10,radii),'TypeError','radius union conversion');
    let closed=false;
    const invalidSequence = {*[Symbol.iterator]() {try {yield 1n;}finally {closed=true;}}};
    throws(() => path.roundRect(0,0,10,10,invalidSequence),'TypeError','sequence conversion rejects BigInt');
    assert(!closed,'Web IDL sequence failure does not close iterator');
    throws(() => path.roundRect(NaN,0,10,10,{x:1n}), 'TypeError',
        'radius conversion before nonfinite rectangle algorithm');
    context.beginPath();context.rect(1,1,2,2);
    context.roundRect(NaN,0,10,10,-1); // finite-coordinate check precedes sign algorithm.
    assert(context.isPointInPath(2,2) && !context.isPointInPath(8,8),'nonfinite geometry leaves path unchanged');
    return {passed:true};
}

function testCanvasSvgPrefix() {
    const context = new OffscreenCanvas(20,20).getContext('2d');
    const reference = new Path2D('M1 1H8V8H1Z');
    for (const suffix of [' X9 9',' L2',' A2 2 0 2 1 4 0',' ?',' M']) {
        const prefix = new Path2D('M1 1H8V8H1Z'+suffix);
        for (const [x,y] of [[3,3],[10,10],[0,0]]) {
            if (context.isPointInPath(prefix,x,y)!==context.isPointInPath(reference,x,y))
                throw Error('malformed suffix lost completed prefix: '+suffix);
        }
    }
    for (const malformed of ['L2 2','M2','X','Z','M0']) {
        if (context.isPointInPath(new Path2D(malformed),1,1)) throw Error('malformed first command painted');
    }
    const repeated = new Path2D('M1 1L8 1 8 8 1 8 1');
    if (!context.isPointInPath(repeated,3,3)) throw Error('repeated command lost completed segments');
    return {passed:true};
}

function testCanvasPathGeometry() {
    const context = new OffscreenCanvas(32,32).getContext('2d');
    for (const [x,y,width,height,hole] of [[5,5,10,10,false],[15,5,-10,10,true],
        [5,15,10,-10,true],[15,15,-10,-10,false]]) {
        const path=new Path2D();path.rect(1,1,18,18);path.roundRect(x,y,width,height,2);
        if(context.isPointInPath(path,10,10)===hole)
            throw Error(`roundRect winding ${width}/${height}`);
        if(!context.isPointInPath(path,2,2))throw Error('roundRect cancelled unrelated geometry');
    }
    context.beginPath();context.roundRect(4,4,12,12,4);
    context.lineTo(2,2);context.lineTo(8,2);context.closePath();
    if(!context.isPointInPath(3,2.5))throw Error('roundRect must begin a fresh subpath at x/y');
    context.beginPath();context.rect(1,1,4,4);context.roundRect(0,0,10,10,[NaN,-1]);
    if(!context.isPointInPath(2,2)||context.isPointInPath(8,8))
        throw Error('nonfinite first radius must stop before negative later radius');
    return {passed:true};
}
