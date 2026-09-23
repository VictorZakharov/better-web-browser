    // CanvasPattern owns a bounded source snapshot and samples through the
    // pattern's inverse matrix. No pixels are painted for nonrepeating regions.
    const canvasPatternToken = Symbol('CanvasPattern');
    class CanvasPattern {
        constructor(token, source, repetition) {
            if (token !== canvasPatternToken) throw new TypeError('Illegal constructor');
            this.__source = source;
            this.__repetition = repetition;
            this.__transform = identity2D();
        }
        setTransform(transform = {}) {
            const values = matrixComponents2D(transform);
            if (values.every(Number.isFinite)) this.__transform = values;
        }
    }
    const sampleCanvasPattern = (pattern, x, y) => {
        const inverse = matrixInverse2D(pattern.__transform);
        if (!inverse) return [0, 0, 0, 0];
        [x, y] = matrixPoint2D(inverse, x, y);
        const source = pattern.__source;
        const repeatX = pattern.__repetition === 'repeat' || pattern.__repetition === 'repeat-x';
        const repeatY = pattern.__repetition === 'repeat' || pattern.__repetition === 'repeat-y';
        if ((!repeatX && (x < 0 || x >= source.width)) ||
            (!repeatY && (y < 0 || y >= source.height))) return [0, 0, 0, 0];
        const column = ((Math.floor(x) % source.width) + source.width) % source.width;
        const row = ((Math.floor(y) % source.height) + source.height) % source.height;
        const offset = (row * source.width + column) * 4;
        return source.pixels.subarray(offset, offset + 4);
    };
    CanvasRenderingContext2D.prototype.createPattern = function(image, repetition = 'repeat') {
        repetition = repetition === null || repetition === '' ? 'repeat' : String(repetition);
        if (!['repeat', 'repeat-x', 'repeat-y', 'no-repeat'].includes(repetition))
            throw new DOMException('Invalid pattern repetition', 'SyntaxError');
        const source = imageSourceSnapshot(image);
        if (!source.width || !source.height) return null;
        return new CanvasPattern(canvasPatternToken, source, repetition);
    };
