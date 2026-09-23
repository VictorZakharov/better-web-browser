    // Geometry Interfaces Level 1 quadrilateral points are mutable, same-object properties.
    // https://www.w3.org/TR/geometry-1/#domquad
    class DOMQuad {
        constructor(p1 = {}, p2 = {}, p3 = {}, p4 = {}) {
            for (const [name, point] of [['p1', p1], ['p2', p2], ['p3', p3], ['p4', p4]])
                Object.defineProperty(this, name, {
                    enumerable: true, value: DOMPoint.fromPoint(point)
                });
        }
        static fromRect(rect = {}) {
            const x = Number(rect.x ?? 0), y = Number(rect.y ?? 0);
            const width = Number(rect.width ?? 0), height = Number(rect.height ?? 0);
            return new this({x, y}, {x: x + width, y},
                {x: x + width, y: y + height}, {x, y: y + height});
        }
        static fromQuad(quad = {}) {
            return new this(quad.p1, quad.p2, quad.p3, quad.p4);
        }
        getBounds() {
            const points = [this.p1, this.p2, this.p3, this.p4];
            const left = Math.min(...points.map(point => point.x));
            const right = Math.max(...points.map(point => point.x));
            const top = Math.min(...points.map(point => point.y));
            const bottom = Math.max(...points.map(point => point.y));
            return new DOMRect(left, top, right - left, bottom - top);
        }
        toJSON() {
            return {p1: this.p1.toJSON(), p2: this.p2.toJSON(),
                p3: this.p3.toJSON(), p4: this.p4.toJSON()};
        }
    }
    globalThis.DOMQuad = DOMQuad;
