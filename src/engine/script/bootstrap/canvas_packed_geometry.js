    // Private Canvas geometry ABI CPG1. Copy numeric points, not JSON point
    // arrays; the existing native provider still applies its f32 boundary and
    // raster budgets. Nothing is exposed on an author-visible prototype.
    const canvasPackGeometry = (() => {
        const Bytes = Uint8Array, View = DataView;
        const apply = Reflect.apply;
        const word = DataView.prototype.setUint32, number = DataView.prototype.setFloat64;
        const buffer = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(Uint8Array.prototype), 'buffer').get;
        return path => {
            const parts = path.subpaths, count = parts.length;
            if (count > 8192) return null;
            let points = 0;
            for (let i = 0; i < count; i++) {
                points += parts[i].points.length;
                if (points > 8192) return null;
            }
            const bytes = new Bytes(8 + count * 8 + points * 16);
            const view = new View(apply(buffer, bytes, []));
            apply(word, view, [0, 0x31475043, true]);
            apply(word, view, [4, count, true]);
            let offset = 8;
            for (let i = 0; i < count; i++) {
                const part = parts[i], vertices = part.points;
                apply(word, view, [offset, vertices.length, true]);
                apply(word, view, [offset + 4, part.closed ? 1 : 0, true]);
                offset += 8;
                for (let j = 0; j < vertices.length; j++) {
                    apply(number, view, [offset, vertices[j][0], true]);
                    apply(number, view, [offset + 8, vertices[j][1], true]);
                    offset += 16;
                }
            }
            return bytes;
        };
    })();
