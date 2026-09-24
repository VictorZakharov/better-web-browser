    // Detached images use the document's Fetch plumbing, but completion requires
    // a successfully decoded image. A completed HTTP response is not a load.
    const detachedImageLoads = new WeakMap();
    const imageDecodeError = () => new DOMException('Image could not be decoded', 'EncodingError');
    const originalImageSetAttribute = HTMLImageElement.prototype.setAttribute;
    const originalImageRemoveAttribute = HTMLImageElement.prototype.removeAttribute;

    const cancelDetachedImage = element => {
        const previous = detachedImageLoads.get(element);
        if (!previous) return;
        previous.controller?.abort();
        previous.settle(false);
        detachedImageLoads.delete(element);
    };
    const loadDetachedImage = element => {
        cancelDetachedImage(element);
        resetImageElementState(element);
        const source = element.getAttribute('src');
        if (source === null || source === '' || element.isConnected) return;
        const controller = new AbortController();
        let settle;
        const promise = new Promise(resolve => { settle = resolve; });
        const state = { controller, promise, settle, decoded: null };
        detachedImageLoads.set(element, state);
        const url = element.src;
        // The decoder only receives bytes visible to this realm. Opaque Fetch
        // responses must not be turned into readable Canvas pixels.
        fetch(url, {
            mode: 'no-cors', credentials: 'include',
            referrerPolicy: 'no-referrer-when-downgrade', signal: controller.signal
        }).then(async response => {
            if (!response.ok && response.type !== 'opaque') throw imageDecodeError();
            const bytes = new Uint8Array(await response.arrayBuffer());
            const decoded = host('canvasDecode', bytes);
            if (!decoded) throw imageDecodeError();
            return decoded;
        }).then(decoded => {
            if (detachedImageLoads.get(element) !== state) return;
            state.decoded = {
                width: Number(decoded[0]), height: Number(decoded[1]),
                pixels: new Uint8ClampedArray(decoded[2])
            };
            updateImageElementState(element, true, decoded[0], decoded[1]);
            state.settle(true);
            if (!element.isConnected) element.dispatchEvent(new Event('load'));
        }, () => {
            if (detachedImageLoads.get(element) !== state) return;
            updateImageElementState(element, true, 0, 0);
            state.settle(false);
            if (!element.isConnected) element.dispatchEvent(new Event('error'));
        });
    };

    HTMLImageElement.prototype.setAttribute = function(name, value) {
        originalImageSetAttribute.call(this, name, value);
        if (String(name).toLowerCase() === 'src') loadDetachedImage(this);
    };
    HTMLImageElement.prototype.removeAttribute = function(name) {
        originalImageRemoveAttribute.call(this, name);
        if (String(name).toLowerCase() === 'src') cancelDetachedImage(this);
    };
    HTMLImageElement.prototype.decode = function() {
        const pending = detachedImageLoads.get(this);
        if (pending) return pending.promise.then(success => {
            if (!success) throw imageDecodeError();
        });
        const image = imageElementState(this);
        return image.complete && image.naturalWidth && image.naturalHeight
            ? Promise.resolve() : Promise.reject(imageDecodeError());
    };
    windowObject.Image = class Image extends HTMLImageElement {
        constructor(width, height) {
            const element = document.createElement('img');
            Object.setPrototypeOf(element, new.target.prototype);
            if (width !== undefined) element.width = Number(width) >>> 0;
            if (height !== undefined) element.height = Number(height) >>> 0;
            return element;
        }
    };
