    // Binding validation remains outside source-layer painting: an empty path
    // must not suppress enum conversion, receiver checks, or overload errors.
    const canvasDrawPathReceiver = context => {
        if (!canvas2dOwners.has(context)) throw new TypeError('Illegal CanvasRenderingContext2D receiver');
    };
    const canvasDrawPathRule = value => {
        if (value === undefined) return 'nonzero';
        const rule = `${value}`;
        if (rule !== 'nonzero' && rule !== 'evenodd') throw new TypeError('Invalid CanvasFillRule');
        return rule;
    };
    const canvasDrawPathBrand = value => {
        if (!canvasPathData.has(value)) throw new TypeError('The path must be a Path2D');
        return value;
    };
    const canvasDrawPathArguments = (name, args) => {
        if (name === 'beginPath') return [];
        if (name === 'stroke') return args.length ? [canvasDrawPathBrand(args[0])] : [];
        if (name === 'fill' || name === 'clip') {
            const external = args.length >= 2 || canvasPathData.has(args[0]);
            return external ? [canvasDrawPathBrand(args[0]), canvasDrawPathRule(args[1])] :
                [canvasDrawPathRule(args[0])];
        }
        const pathHit = name === 'isPointInPath';
        if (args.length < 2) throw new TypeError(`${name} requires coordinates`);
        const external = pathHit ? args.length >= 4 ||
            (args.length >= 3 && canvasPathData.has(args[0])) : args.length >= 3;
        const converted = external ? [canvasDrawPathBrand(args[0])] : [];
        const offset = external ? 1 : 0;
        converted.push(+args[offset], +args[offset + 1]);
        if (pathHit) converted.push(canvasDrawPathRule(args[offset + 2]));
        return converted;
    };
    for (const name of ['beginPath','fill','stroke','clip','isPointInPath','isPointInStroke']) {
        const implementation = CanvasRenderingContext2D.prototype[name];
        const method = function(...args) {
            canvasDrawPathReceiver(this);
            return canvasPathApply(implementation, this, canvasDrawPathArguments(name, args));
        };
        canvasPathDefine(method, 'name', {value:name, configurable:true});
        canvasPathDefine(method, 'length', {value:name.startsWith('isPoint') ? 2 : 0, configurable:true});
        canvasPathDefine(CanvasRenderingContext2D.prototype, name,
            {value:method, enumerable:true, writable:true, configurable:true});
    }
    bindCanvasPathNumbers(CanvasRenderingContext2D.prototype,
        [['clearRect',4],['fillRect',4],['strokeRect',4]]);
