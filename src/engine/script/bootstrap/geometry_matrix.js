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
