    // Workers expose Geometry Interfaces without browser layout or DOM nodes.
    class DOMRectReadOnly {
        constructor(x = 0, y = 0, width = 0, height = 0) {
            this.__rectangle = [Number(x), Number(y), Number(width), Number(height)];
        }
        static fromRect(rect = {}) {
            return new this(rect.x ?? 0, rect.y ?? 0, rect.width ?? 0, rect.height ?? 0);
        }
        get x() { return this.__rectangle[0]; }
        get y() { return this.__rectangle[1]; }
        get width() { return this.__rectangle[2]; }
        get height() { return this.__rectangle[3]; }
        get top() { return Math.min(this.y, this.y + this.height); }
        get right() { return Math.max(this.x, this.x + this.width); }
        get bottom() { return Math.max(this.y, this.y + this.height); }
        get left() { return Math.min(this.x, this.x + this.width); }
        toJSON() { return {x: this.x, y: this.y, width: this.width, height: this.height,
            top: this.top, right: this.right, bottom: this.bottom, left: this.left}; }
    }
    class DOMRect extends DOMRectReadOnly {}
    for (const [index, name] of ['x', 'y', 'width', 'height'].entries())
        Object.defineProperty(DOMRect.prototype, name, {
            configurable: true, enumerable: true,
            get() { return this.__rectangle[index]; },
            set(value) { this.__rectangle[index] = Number(value); }
        });
    Object.assign(globalThis, {DOMRect, DOMRectReadOnly});
