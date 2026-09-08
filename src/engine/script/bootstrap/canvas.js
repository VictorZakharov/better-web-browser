    // This first Canvas 2D slice owns a bounded software bitmap. It intentionally exposes only
    // operations backed by real pixels; unsupported context types and APIs continue to fail closed.
    const MAX_CANVAS_PIXELS = 4 * 1024 * 1024;
    const canvasStates = new WeakMap();

    const canvasDimension = (element, name, fallback) => {
        const raw = element.getAttribute(name);
        if (raw === null || raw.trim() === '' || !/^\d+$/.test(raw.trim())) return fallback;
        return Math.min(0xffffffff, Number(raw));
    };

    class ImageData {
        constructor(dataOrWidth, widthOrHeight, heightOrSettings, settings = {}) {
            let data;
            let width;
            let height;
            if (dataOrWidth instanceof Uint8ClampedArray) {
                data = dataOrWidth;
                width = Math.trunc(Number(widthOrHeight));
                height = heightOrSettings === undefined || typeof heightOrSettings === 'object'
                    ? data.length / 4 / width
                    : Math.trunc(Number(heightOrSettings));
                settings = (typeof heightOrSettings === 'object' ? heightOrSettings : settings) || {};
                if (width <= 0 || height <= 0 || !Number.isInteger(height) ||
                    data.length !== width * height * 4)
                    throw new DOMException('ImageData dimensions do not match its data', 'IndexSizeError');
                if (width * height > MAX_CANVAS_PIXELS)
                    throw new DOMException('ImageData exceeds the bitmap budget', 'NotSupportedError');
            } else {
                width = Math.trunc(Number(dataOrWidth));
                height = Math.trunc(Number(widthOrHeight));
                settings = heightOrSettings || {};
                if (width <= 0 || height <= 0)
                    throw new DOMException('ImageData dimensions must be positive', 'IndexSizeError');
                if (width * height > MAX_CANVAS_PIXELS)
                    throw new DOMException('ImageData exceeds the bitmap budget', 'NotSupportedError');
                data = new Uint8ClampedArray(width * height * 4);
            }
            if (settings.colorSpace !== undefined && settings.colorSpace !== 'srgb')
                throw new TypeError('Only the srgb ImageData color space is supported');
            Object.defineProperties(this, {
                data: { enumerable: true, value: data },
                width: { enumerable: true, value: width },
                height: { enumerable: true, value: height },
                colorSpace: { enumerable: true, value: 'srgb' }
            });
        }
    }

    const normalizedColor = value => {
        const result = host('normalizeCssColor', String(value));
        if (!result) return null;
        const [serialized, red, green, blue, alpha] = result.split('\u001f');
        return { serialized, channels: [Number(red), Number(green), Number(blue), Number(alpha)] };
    };

    const stateForCanvas = canvas => {
        const width = canvas.width;
        const height = canvas.height;
        let state = canvasStates.get(canvas);
        if (!state) {
            state = { width: -1, height: -1, pixels: null, context: null };
            canvasStates.set(canvas, state);
        }
        if (state.width !== width || state.height !== height) {
            state.width = width;
            state.height = height;
            state.pixels = width * height <= MAX_CANVAS_PIXELS
                ? new Uint8ClampedArray(width * height * 4)
                : null;
            if (state.context) state.context.__reset();
        }
        return state;
    };

    const normalizedRectangle = (x, y, width, height) => {
        x = Math.trunc(Number(x));
        y = Math.trunc(Number(y));
        width = Math.trunc(Number(width));
        height = Math.trunc(Number(height));
        if (![x, y, width, height].every(Number.isFinite)) return null;
        if (width < 0) { x += width; width = -width; }
        if (height < 0) { y += height; height = -height; }
        return { x, y, width, height };
    };

    class CanvasRenderingContext2D {
        constructor(canvas) {
            Object.defineProperty(this, 'canvas', { enumerable: true, value: canvas });
            this.__reset();
        }
        __reset() {
            this.__fill = normalizedColor('#000000');
            this.__globalAlpha = 1;
            this.__stack = [];
        }
        get fillStyle() { return this.__fill.serialized; }
        set fillStyle(value) {
            const color = normalizedColor(value);
            if (color) this.__fill = color;
        }
        get globalAlpha() { return this.__globalAlpha; }
        set globalAlpha(value) {
            value = Number(value);
            if (Number.isFinite(value) && value >= 0 && value <= 1) this.__globalAlpha = value;
        }
        save() {
            if (this.__stack.length < 64)
                this.__stack.push({ fill: this.__fill, globalAlpha: this.__globalAlpha });
        }
        restore() {
            const state = this.__stack.pop();
            if (state) { this.__fill = state.fill; this.__globalAlpha = state.globalAlpha; }
        }
        clearRect(x, y, width, height) { this.__paintRect(x, y, width, height, null); }
        fillRect(x, y, width, height) { this.__paintRect(x, y, width, height, this.__fill.channels); }
        __paintRect(x, y, width, height, color) {
            const rect = normalizedRectangle(x, y, width, height);
            const state = stateForCanvas(this.canvas);
            if (!rect || !state.pixels) return;
            const left = Math.max(0, rect.x);
            const top = Math.max(0, rect.y);
            const right = Math.min(state.width, rect.x + rect.width);
            const bottom = Math.min(state.height, rect.y + rect.height);
            const sourceAlpha = color ? color[3] / 255 * this.__globalAlpha : 0;
            for (let row = top; row < bottom; row++) for (let column = left; column < right; column++) {
                const offset = (row * state.width + column) * 4;
                if (!color) {
                    state.pixels.fill(0, offset, offset + 4);
                    continue;
                }
                const destinationAlpha = state.pixels[offset + 3] / 255;
                const outputAlpha = sourceAlpha + destinationAlpha * (1 - sourceAlpha);
                for (let channel = 0; channel < 3; channel++) {
                    const value = outputAlpha === 0 ? 0 :
                        (color[channel] * sourceAlpha + state.pixels[offset + channel] *
                            destinationAlpha * (1 - sourceAlpha)) / outputAlpha;
                    state.pixels[offset + channel] = Math.round(value);
                }
                state.pixels[offset + 3] = Math.round(outputAlpha * 255);
            }
        }
        createImageData(widthOrImageData, height, settings) {
            if (widthOrImageData instanceof ImageData)
                return new ImageData(widthOrImageData.width, widthOrImageData.height, height);
            return new ImageData(Math.abs(Number(widthOrImageData)), Math.abs(Number(height)), settings);
        }
        getImageData(x, y, width, height, settings) {
            const rect = normalizedRectangle(x, y, width, height);
            if (!rect || rect.width === 0 || rect.height === 0)
                throw new DOMException('ImageData dimensions must be non-zero', 'IndexSizeError');
            const state = stateForCanvas(this.canvas);
            if (!state.pixels || rect.width * rect.height > MAX_CANVAS_PIXELS)
                throw new DOMException('The requested bitmap exceeds the canvas budget', 'NotSupportedError');
            const result = new ImageData(rect.width, rect.height, settings);
            for (let row = 0; row < rect.height; row++) for (let column = 0; column < rect.width; column++) {
                const sourceX = rect.x + column;
                const sourceY = rect.y + row;
                if (sourceX < 0 || sourceY < 0 || sourceX >= state.width || sourceY >= state.height) continue;
                const source = (sourceY * state.width + sourceX) * 4;
                const destination = (row * rect.width + column) * 4;
                result.data.set(state.pixels.subarray(source, source + 4), destination);
            }
            return result;
        }
        putImageData(imageData, x, y) {
            if (!(imageData instanceof ImageData)) throw new TypeError('putImageData requires ImageData');
            const state = stateForCanvas(this.canvas);
            if (!state.pixels) return;
            x = Math.trunc(Number(x));
            y = Math.trunc(Number(y));
            for (let row = 0; row < imageData.height; row++) for (let column = 0; column < imageData.width; column++) {
                const destinationX = x + column;
                const destinationY = y + row;
                if (destinationX < 0 || destinationY < 0 || destinationX >= state.width || destinationY >= state.height) continue;
                const source = (row * imageData.width + column) * 4;
                const destination = (destinationY * state.width + destinationX) * 4;
                state.pixels.set(imageData.data.subarray(source, source + 4), destination);
            }
        }
        getContextAttributes() { return { alpha: true, colorSpace: 'srgb', desynchronized: false, willReadFrequently: false }; }
        isContextLost() { return false; }
    }

    class HTMLCanvasElement extends HTMLElement {
        get width() { return canvasDimension(this, 'width', 300); }
        set width(value) {
            this.setAttribute('width', Math.max(0, Math.trunc(Number(value))) || 0);
            stateForCanvas(this);
        }
        get height() { return canvasDimension(this, 'height', 150); }
        set height(value) {
            this.setAttribute('height', Math.max(0, Math.trunc(Number(value))) || 0);
            stateForCanvas(this);
        }
        getContext(contextId) {
            if (String(contextId).toLowerCase() !== '2d') return null;
            const state = stateForCanvas(this);
            return state.context ||= new CanvasRenderingContext2D(this);
        }
    }
