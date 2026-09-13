    // Geometry Interfaces / Resize Observer: native layout snapshots are copied into branded,
    // read-only objects. Author mutation cannot change the observer's last-reported size.
    const resizeEntries = new WeakMap(), resizeSizes = new WeakMap(), domRects = new WeakMap();
    const branded = (map, value) => {
        const state = map.get(value);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    const interfacePrototype = (type, fields, state) => {
        for (const field of fields) Object.defineProperty(type.prototype, field, {
            enumerable: true, configurable: true, get() { return branded(state, this)[field]; }
        });
        Object.defineProperty(type.prototype, Symbol.toStringTag, {value: type.name, configurable: true});
    };
    class DOMRectReadOnly {
        constructor(x = 0, y = 0, width = 0, height = 0) {
            domRects.set(this, {x: +x, y: +y, width: +width, height: +height});
        }
        static fromRect(other = {}) {
            if (other != null && typeof other !== 'object' && typeof other !== 'function')
                throw new TypeError('DOMRectInit must be a dictionary');
            return new DOMRectReadOnly(other?.x, other?.y, other?.width, other?.height);
        }
        get top() { const r = branded(domRects, this); return Math.min(r.y, r.y + r.height); }
        get right() { const r = branded(domRects, this); return Math.max(r.x, r.x + r.width); }
        get bottom() { const r = branded(domRects, this); return Math.max(r.y, r.y + r.height); }
        get left() { const r = branded(domRects, this); return Math.min(r.x, r.x + r.width); }
        toJSON() {
            const r = branded(domRects, this);
            return {...r, top: Math.min(r.y, r.y + r.height), right: Math.max(r.x, r.x + r.width),
                bottom: Math.max(r.y, r.y + r.height), left: Math.min(r.x, r.x + r.width)};
        }
    }
    interfacePrototype(DOMRectReadOnly, ['x', 'y', 'width', 'height'], domRects);
    for (const field of ['top', 'right', 'bottom', 'left', 'toJSON'])
        Object.defineProperty(DOMRectReadOnly.prototype, field,
            {...Object.getOwnPropertyDescriptor(DOMRectReadOnly.prototype, field), enumerable: true});
    class ResizeObserverSize { constructor() { throw new TypeError('Illegal constructor'); } }
    class ResizeObserverEntry { constructor() { throw new TypeError('Illegal constructor'); } }
    interfacePrototype(ResizeObserverSize, ['inlineSize', 'blockSize'], resizeSizes);
    interfacePrototype(ResizeObserverEntry,
        ['target', 'contentRect', 'contentBoxSize', 'borderBoxSize', 'devicePixelContentBoxSize'], resizeEntries);
    const resizeSize = (inlineSize, blockSize) => {
        const size = Object.create(ResizeObserverSize.prototype);
        resizeSizes.set(size, {inlineSize, blockSize});
        return Object.freeze([size]);
    };
    const resizeEntry = (target, box) => {
        const entry = Object.create(ResizeObserverEntry.prototype);
        resizeEntries.set(entry, {
            target, contentRect: new DOMRectReadOnly(box[0], box[1], box[2], box[3]),
            contentBoxSize: resizeSize(box[2], box[3]), borderBoxSize: resizeSize(box[4], box[5]),
            devicePixelContentBoxSize: resizeSize(Math.round(box[2] * box[6]), Math.round(box[3] * box[6]))
        });
        return entry;
    };
    Object.assign(windowObject, {DOMRectReadOnly, ResizeObserverSize, ResizeObserverEntry});
