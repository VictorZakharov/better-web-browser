    // CanvasPattern owns a bounded source snapshot and samples through the
    // pattern's inverse matrix. No pixels are painted for nonrepeating regions.
    const canvasPatternToken = Symbol('CanvasPattern');
    const canvasPatternStates = new WeakMap();
    const canvasPatternGet = Function.call.bind(WeakMap.prototype.get);
    const canvasPatternSet = Function.call.bind(WeakMap.prototype.set);
    const canvasPatternHas = Function.call.bind(WeakMap.prototype.has);
    const canvasPatternSlice = Function.call.bind(Uint8ClampedArray.prototype.subarray);
    const canvasIsPattern = value => canvasPatternHas(canvasPatternStates,value);
    class CanvasPattern {
        constructor(token, source, repetition) {
            if (token !== canvasPatternToken) throw new TypeError('Illegal constructor');
            canvasPatternSet(canvasPatternStates,this,{source,repetition,transform:identity2D()});
        }
        setTransform(transform = {}) {
            const state=canvasPatternGet(canvasPatternStates,this);
            if(!state)throw new TypeError('setTransform requires a CanvasPattern');
            const values = matrixDictionary2D(transform);
            if (values.every(Number.isFinite)) state.transform = values;
        }
    }
    Object.defineProperty(CanvasPattern.prototype,Symbol.toStringTag,{value:'CanvasPattern',configurable:true});
    Object.defineProperty(CanvasPattern.prototype,'setTransform',{enumerable:true});
    const sampleCanvasPattern = (pattern, x, y) => {
        const state=canvasPatternGet(canvasPatternStates,pattern);
        const inverse = matrixInverse2D(state.transform);
        if (!inverse) return [0, 0, 0, 0];
        [x, y] = matrixPoint2D(inverse, x, y);
        const source = state.source;
        const repeatX = state.repetition === 'repeat' || state.repetition === 'repeat-x';
        const repeatY = state.repetition === 'repeat' || state.repetition === 'repeat-y';
        if ((!repeatX && (x < 0 || x >= source.width)) ||
            (!repeatY && (y < 0 || y >= source.height))) return [0, 0, 0, 0];
        const column = ((canvasPrivateMath.floor(x) % source.width) + source.width) % source.width;
        const row = ((canvasPrivateMath.floor(y) % source.height) + source.height) % source.height;
        const offset = (row * source.width + column) * 4;
        return canvasPatternSlice(source.pixels,offset,offset + 4);
    };
    CanvasRenderingContext2D.prototype.createPattern = function createPattern(image, repetition) {
        canvasImageDataContext(this);
        if(arguments.length<2)throw new TypeError('createPattern requires two arguments');
        // Convert the image interface before DOMString; usability/snapshot comes
        // afterward, so later argument getters may still change source pixels.
        if(!canvasImageSourceSupported(image))throw new TypeError('Unsupported Canvas image source');
        repetition = repetition === null ? '' : `${repetition}`;
        if(!canvasImageSourceUsable(image))return null;
        const source = imageSourceSnapshot(image, false, true);
        if (!source.width || !source.height) return null;
        if(repetition==='')repetition='repeat';
        if (!['repeat', 'repeat-x', 'repeat-y', 'no-repeat'].includes(repetition))
            throw new DOMException('Invalid pattern repetition', 'SyntaxError');
        return new CanvasPattern(canvasPatternToken, source, repetition);
    };
