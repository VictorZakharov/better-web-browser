    class DOMPointReadOnly {
        constructor(x = 0, y = 0, z = 0, w = 1) {
            this.__coordinates = [Number(x), Number(y), Number(z), Number(w)];
        }
        get x() { return this.__coordinates[0]; }
        get y() { return this.__coordinates[1]; }
        get z() { return this.__coordinates[2]; }
        get w() { return this.__coordinates[3]; }
        static fromPoint(point = {}) {
            return new this(point.x ?? 0, point.y ?? 0, point.z ?? 0, point.w ?? 1);
        }
        matrixTransform(matrix = {}) {
            return new DOMPoint(...matrixPoint4(matrix4FromSource(matrix).values, this.__coordinates));
        }
        toJSON() { return { x: this.x, y: this.y, z: this.z, w: this.w }; }
    }
    class DOMPoint extends DOMPointReadOnly {}
    for (const [index, name] of ['x', 'y', 'z', 'w'].entries())
        Object.defineProperty(DOMPoint.prototype, name, {
            configurable: true, enumerable: true,
            get() { return this.__coordinates[index]; },
            set(value) { this.__coordinates[index] = Number(value); }
        });
    class DOMMatrixReadOnly {
        constructor(init) {
            const parsed = matrix4FromSource(init);
            this.__values = parsed.values;
            this.__is2D = parsed.is2D;
        }
        static fromMatrix(init = {}) { return new this(init); }
        static fromFloat32Array(array) { return new this(array); }
        static fromFloat64Array(array) { return new this(array); }
        get is2D() { return this.__is2D; }
        get isIdentity() { return this.__values.every((value, index) => value === identity4()[index]); }
        multiply(other) { return DOMMatrix.fromMatrix(this).multiplySelf(other); }
        translate(tx = 0, ty = 0, tz = 0) {
            return DOMMatrix.fromMatrix(this).translateSelf(tx, ty, tz);
        }
        scale(scaleX = 1, scaleY = scaleX, scaleZ = 1, originX = 0, originY = 0, originZ = 0) {
            return DOMMatrix.fromMatrix(this).scaleSelf(scaleX, scaleY, scaleZ, originX, originY, originZ);
        }
        scale3d(scale = 1, originX = 0, originY = 0, originZ = 0) {
            return this.scale(scale, scale, scale, originX, originY, originZ);
        }
        rotate(rotX = 0, rotY, rotZ) { return DOMMatrix.fromMatrix(this).rotateSelf(rotX, rotY, rotZ); }
        rotateFromVector(x = 0, y = 0) {
            return DOMMatrix.fromMatrix(this).rotateFromVectorSelf(x, y);
        }
        rotateAxisAngle(x = 0, y = 0, z = 0, angle = 0) {
            return DOMMatrix.fromMatrix(this).rotateAxisAngleSelf(x, y, z, angle);
        }
        skewX(angle = 0) { return DOMMatrix.fromMatrix(this).skewXSelf(angle); }
        skewY(angle = 0) { return DOMMatrix.fromMatrix(this).skewYSelf(angle); }
        flipX() { return this.scale(-1, 1); }
        flipY() { return this.scale(1, -1); }
        inverse() { return DOMMatrix.fromMatrix(this).invertSelf(); }
        transformPoint(point = {}) {
            return DOMPointReadOnly.fromPoint(point).matrixTransform(this);
        }
        toFloat32Array() { return Float32Array.from(this.__values); }
        toFloat64Array() { return Float64Array.from(this.__values); }
        toJSON() {
            const result = { a: this.a, b: this.b, c: this.c, d: this.d,
                e: this.e, f: this.f, is2D: this.is2D, isIdentity: this.isIdentity };
            for (let index = 0; index < 16; index++)
                result[`m${Math.floor(index / 4) + 1}${index % 4 + 1}`] = this.__values[index];
            return result;
        }
        toString() {
            return this.is2D ? `matrix(${matrix4To2D(this.__values).join(', ')})` :
                `matrix3d(${this.__values.join(', ')})`;
        }
    }
    class DOMMatrix extends DOMMatrixReadOnly {
        multiplySelf(other) {
            const parsed = matrix4FromSource(other);
            this.__values = matrixMultiply4(this.__values, parsed.values);
            this.__is2D &&= parsed.is2D;
            return this;
        }
        preMultiplySelf(other) {
            const parsed = matrix4FromSource(other);
            this.__values = matrixMultiply4(parsed.values, this.__values);
            this.__is2D &&= parsed.is2D;
            return this;
        }
        translateSelf(tx = 0, ty = 0, tz = 0) {
            tz = Number(tz);
            this.__values = matrixMultiply4(this.__values,
                matrixTranslation4(Number(tx), Number(ty), tz));
            if (tz !== 0) this.__is2D = false;
            return this;
        }
        scaleSelf(scaleX = 1, scaleY = scaleX, scaleZ = 1,
            originX = 0, originY = 0, originZ = 0) {
            [scaleX, scaleY, scaleZ, originX, originY, originZ] =
                [scaleX, scaleY, scaleZ, originX, originY, originZ].map(Number);
            this.translateSelf(originX, originY, originZ);
            this.__values = matrixMultiply4(this.__values,
                matrixScale4(scaleX, scaleY, scaleZ));
            if (scaleZ !== 1 || originZ !== 0) this.__is2D = false;
            return this.translateSelf(-originX, -originY, -originZ);
        }
        scale3dSelf(scale = 1, originX = 0, originY = 0, originZ = 0) {
            return this.scaleSelf(scale, scale, scale, originX, originY, originZ);
        }
        rotateSelf(rotX = 0, rotY, rotZ) {
            if (rotY === undefined && rotZ === undefined) {
                rotZ = rotX; rotX = 0; rotY = 0;
            }
            rotY ??= 0; rotZ ??= 0;
            for (const [axis, angle] of [[[0, 0, 1], rotZ], [[0, 1, 0], rotY],
                [[1, 0, 0], rotX]]) {
                this.__values = matrixMultiply4(this.__values,
                    matrixRotation4(...axis, Number(angle)));
            }
            if (Number(rotX) !== 0 || Number(rotY) !== 0) this.__is2D = false;
            return this;
        }
        rotateFromVectorSelf(x = 0, y = 0) {
            return this.rotateSelf(Math.atan2(Number(y), Number(x)) * 180 / Math.PI);
        }
        rotateAxisAngleSelf(x = 0, y = 0, z = 0, angle = 0) {
            this.__values = matrixMultiply4(this.__values,
                matrixRotation4(Number(x), Number(y), Number(z), Number(angle)));
            if (Number(x) !== 0 || Number(y) !== 0) this.__is2D = false;
            return this;
        }
        skewXSelf(angle = 0) {
            const matrix = identity4(); matrix[4] = Math.tan(Number(angle) * Math.PI / 180);
            this.__values = matrixMultiply4(this.__values, matrix); return this;
        }
        skewYSelf(angle = 0) {
            const matrix = identity4(); matrix[1] = Math.tan(Number(angle) * Math.PI / 180);
            this.__values = matrixMultiply4(this.__values, matrix); return this;
        }
        invertSelf() {
            const inverse = matrixInverse4(this.__values);
            if (inverse) this.__values = inverse;
            else { this.__values = new Array(16).fill(NaN); this.__is2D = false; }
            return this;
        }
        setMatrixValue(text) {
            const parsed = matrix4FromSource(String(text));
            this.__values = parsed.values; this.__is2D = parsed.is2D;
            return this;
        }
    }
    const matrixAliases = { a: 0, b: 1, c: 4, d: 5, e: 12, f: 13 };
    for (let index = 0; index < 16; index++) {
        const name = `m${Math.floor(index / 4) + 1}${index % 4 + 1}`;
        Object.defineProperty(DOMMatrixReadOnly.prototype, name, {
            configurable: true, enumerable: true, get() { return this.__values[index]; }
        });
        Object.defineProperty(DOMMatrix.prototype, name, {
            configurable: true, enumerable: true,
            get() { return this.__values[index]; },
            set(value) {
                this.__values[index] = Number(value);
                if (![0, 1, 4, 5, 12, 13].includes(index)) this.__is2D = false;
            }
        });
    }
    for (const [alias, index] of Object.entries(matrixAliases)) {
        Object.defineProperty(DOMMatrixReadOnly.prototype, alias, {
            configurable: true, enumerable: true, get() { return this.__values[index]; }
        });
        Object.defineProperty(DOMMatrix.prototype, alias, {
            configurable: true, enumerable: true,
            get() { return this.__values[index]; },
            set(value) { this.__values[index] = Number(value); }
        });
    }
    Object.assign(globalThis, { DOMPoint, DOMPointReadOnly, DOMMatrix, DOMMatrixReadOnly });
