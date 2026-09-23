    // Geometry Interfaces Level 1 uses column-major 4x4 matrices and column vectors.
    // https://www.w3.org/TR/geometry-1/#matrix-math
    const identity4 = () => [1, 0, 0, 0, 0, 1, 0, 0,
        0, 0, 1, 0, 0, 0, 0, 1];
    const matrix4From2D = m => [m[0], m[1], 0, 0, m[2], m[3], 0, 0,
        0, 0, 1, 0, m[4], m[5], 0, 1];
    const matrix4To2D = m => [m[0], m[1], m[4], m[5], m[12], m[13]];
    const matrix4Is2D = m => [m[2], m[3], m[6], m[7], m[8], m[9], m[11], m[14]]
        .every(value => value === 0) && m[10] === 1 && m[15] === 1;
    const matrixDictionaryIs3D = source => source.is2D === false ||
        ['m13', 'm14', 'm23', 'm24', 'm31', 'm32', 'm33', 'm34',
            'm43', 'm44'].some(name => source[name] !== undefined);
    const matrix4FromDictionary = source => {
        const values = identity4();
        for (let index = 0; index < 16; index++) {
            const column = Math.floor(index / 4) + 1, row = index % 4 + 1;
            const value = source[`m${column}${row}`];
            if (value !== undefined) values[index] = Number(value);
        }
        for (const [alias, index] of [['a', 0], ['b', 1], ['c', 4],
            ['d', 5], ['e', 12], ['f', 13]]) {
            if (source[alias] === undefined) continue;
            const value = Number(source[alias]);
            const long = `m${Math.floor(index / 4) + 1}${index % 4 + 1}`;
            if (source[long] !== undefined && values[index] !== value)
                throw new TypeError('Conflicting matrix component aliases');
            values[index] = value;
        }
        return { values, is2D: !matrixDictionaryIs3D(source) && matrix4Is2D(values) };
    };
    const matrix4FromSource = source => {
        if (source === undefined || source === null) return { values: identity4(), is2D: true };
        if (source instanceof DOMMatrixReadOnly)
            return { values: [...source.__values], is2D: source.is2D };
        if (typeof source === 'string') {
            const text = source.trim();
            const match = /^(matrix|matrix3d)\(\s*([^)]*)\s*\)$/i.exec(text);
            if (!match) throw new DOMException('Invalid matrix string', 'SyntaxError');
            const values = match[2].split(/\s*,\s*/).map(Number);
            const is2D = match[1].toLowerCase() === 'matrix';
            if (values.length !== (is2D ? 6 : 16) || !values.every(Number.isFinite))
                throw new DOMException('Invalid matrix string', 'SyntaxError');
            return { values: is2D ? matrix4From2D(values) : values, is2D };
        }
        if (typeof source !== 'object') throw new TypeError('Invalid matrix initializer');
        if (typeof source[Symbol.iterator] === 'function') {
            const values = Array.from(source, Number);
            if (values.length !== 6 && values.length !== 16)
                throw new TypeError('Matrix sequence must have six or sixteen entries');
            return { values: values.length === 6 ? matrix4From2D(values) : values,
                is2D: values.length === 6 };
        }
        return matrix4FromDictionary(source);
    };
    const matrixMultiply4 = (left, right) => {
        const result = new Array(16).fill(0);
        for (let column = 0; column < 4; column++) for (let row = 0; row < 4; row++)
            for (let k = 0; k < 4; k++)
                result[column * 4 + row] += left[k * 4 + row] * right[column * 4 + k];
        return result;
    };
    const matrixPoint4 = (matrix, point) => [0, 1, 2, 3].map(row =>
        matrix[row] * point[0] + matrix[4 + row] * point[1] +
        matrix[8 + row] * point[2] + matrix[12 + row] * point[3]);
    const matrixInverse4 = matrix => {
        const rows = Array.from({length: 4}, (_, row) => [
            ...Array.from({length: 4}, (_, column) => matrix[column * 4 + row]),
            ...Array.from({length: 4}, (_, column) => Number(row === column))
        ]);
        for (let column = 0; column < 4; column++) {
            let pivot = column;
            for (let row = column + 1; row < 4; row++)
                if (Math.abs(rows[row][column]) > Math.abs(rows[pivot][column])) pivot = row;
            if (!Number.isFinite(rows[pivot][column]) || rows[pivot][column] === 0) return null;
            [rows[column], rows[pivot]] = [rows[pivot], rows[column]];
            const divisor = rows[column][column];
            for (let index = 0; index < 8; index++) rows[column][index] /= divisor;
            for (let row = 0; row < 4; row++) if (row !== column) {
                const factor = rows[row][column];
                for (let index = 0; index < 8; index++)
                    rows[row][index] -= factor * rows[column][index];
            }
        }
        return Array.from({length: 16}, (_, index) =>
            rows[index % 4][4 + Math.floor(index / 4)]);
    };
    const matrixTranslation4 = (x, y, z) => {
        const values = identity4(); values[12] = x; values[13] = y; values[14] = z;
        return values;
    };
    const matrixScale4 = (x, y, z) => {
        const values = identity4(); values[0] = x; values[5] = y; values[10] = z;
        return values;
    };
    const matrixRotation4 = (x, y, z, degrees) => {
        const length = Math.hypot(x, y, z);
        if (!length) return identity4();
        x /= length; y /= length; z /= length;
        const angle = degrees * Math.PI / 180, cosine = Math.cos(angle);
        const sine = Math.sin(angle), inverse = 1 - cosine;
        return [
            x*x*inverse + cosine, y*x*inverse + z*sine, z*x*inverse - y*sine, 0,
            x*y*inverse - z*sine, y*y*inverse + cosine, z*y*inverse + x*sine, 0,
            x*z*inverse + y*sine, y*z*inverse - x*sine, z*z*inverse + cosine, 0,
            0, 0, 0, 1
        ];
    };
