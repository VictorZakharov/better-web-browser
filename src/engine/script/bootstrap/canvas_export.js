    // HTML §4.12.5: serialize the canvas's owned bitmap, never an empty stand-in.
    // https://html.spec.whatwg.org/multipage/canvas.html#dom-canvas-todataurl-dev
    const encodedCanvas = (canvas, type, quality) => {
        const owned = stateForCanvas(canvas);
        const state = owned.placeholder ? stateForCanvas(owned.placeholder) : owned;
        if (!state.width || !state.height || !state.pixels) return null;
        return host('canvasEncode', state.width, state.height, type, quality, state.pixels);
    };
    const base64Canvas = bytes => {
        let binary = '';
        for (let start = 0; start < bytes.length; start += 0x4000)
            binary += String.fromCharCode(...bytes.subarray(start, start + 0x4000));
        return btoa(binary);
    };
    Object.defineProperties(HTMLCanvasElement.prototype, {
        toDataURL: {
            configurable: true, enumerable: true,
            value: function(type = 'image/png', quality) {
                if (!(this instanceof HTMLCanvasElement)) throw new TypeError('Illegal canvas receiver');
                const encoded = encodedCanvas(this, String(type), quality);
                if (!encoded) return 'data:,';
                return 'data:' + encoded[0] + ';base64,' + base64Canvas(encoded[1]);
            }
        },
        toBlob: {
            configurable: true, enumerable: true,
            value: function(callback, type = 'image/png', quality) {
                if (!(this instanceof HTMLCanvasElement)) throw new TypeError('Illegal canvas receiver');
                if (typeof callback !== 'function') throw new TypeError('toBlob requires a callback');
                const encoded = encodedCanvas(this, String(type), quality);
                const blob = encoded ? new Blob([encoded[1]], { type: encoded[0] }) : null;
                setTimeout(() => callback(blob), 0);
            }
        }
    });
