    // Canvas 2D stores the affine subset in column-vector order [a,b,c,d,e,f].
    // DOMMatrix itself is implemented in the following Geometry Interfaces modules.
    const identity2D = () => [1, 0, 0, 1, 0, 0];
    const matrixMultiply2D = (left, right) => [
        left[0] * right[0] + left[2] * right[1],
        left[1] * right[0] + left[3] * right[1],
        left[0] * right[2] + left[2] * right[3],
        left[1] * right[2] + left[3] * right[3],
        left[0] * right[4] + left[2] * right[5] + left[4],
        left[1] * right[4] + left[3] * right[5] + left[5]
    ];
    const matrixInverse2D = matrix => {
        const [a, b, c, d, e, f] = matrix;
        const determinant = a * d - b * c;
        if (!Number.isFinite(determinant) || determinant === 0) return null;
        return [d / determinant, -b / determinant, -c / determinant, a / determinant,
            (c * f - d * e) / determinant, (b * e - a * f) / determinant];
    };
    const matrixPoint2D = (matrix, x, y) => [
        matrix[0] * x + matrix[2] * y + matrix[4],
        matrix[1] * x + matrix[3] * y + matrix[5]
    ];
    const matrixComponents2D = source => {
        if (source === undefined || source === null) return identity2D();
        if (source instanceof DOMMatrixReadOnly) {
            if (!source.is2D) throw new TypeError('Canvas requires a 2D matrix');
            return [source.a, source.b, source.c, source.d, source.e, source.f];
        }
        if (typeof source === 'string') {
            const match = /^matrix\(\s*([^)]*)\s*\)$/i.exec(source.trim());
            if (!match) throw new DOMException('Invalid 2D matrix string', 'SyntaxError');
            const values = match[1].split(/\s*,\s*/).map(Number);
            if (values.length !== 6 || !values.every(Number.isFinite))
                throw new DOMException('Invalid 2D matrix string', 'SyntaxError');
            return values;
        }
        if (typeof source[Symbol.iterator] === 'function') {
            const values = Array.from(source, Number);
            if (values.length !== 6) throw new TypeError('A 2D matrix sequence must have six entries');
            return values;
        }
        if (typeof source !== 'object') throw new TypeError('Invalid matrix initializer');
        if (matrixDictionaryIs3D(source)) throw new TypeError('Canvas requires a 2D matrix');
        const aliases = [['a', 'm11'], ['b', 'm12'], ['c', 'm21'],
            ['d', 'm22'], ['e', 'm41'], ['f', 'm42']];
        return aliases.map(([short, long], index) => {
            const fallback = index === 0 || index === 3 ? 1 : 0;
            const left = source[short] === undefined ? undefined : Number(source[short]);
            const right = source[long] === undefined ? undefined : Number(source[long]);
            if (left !== undefined && right !== undefined && left !== right)
                throw new TypeError('Conflicting matrix component aliases');
            return left ?? right ?? fallback;
        });
    };
