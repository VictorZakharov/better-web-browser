(() => {
    'use strict';
    // Beacon §3: the method queues a POST; its return value says nothing about delivery.
    // Keep serialization synchronous so a page can queue a beacon during unload.
    const data = globalThis.__networkData;
    const urlApi = globalThis.__urlInternals;
    const MAX_KEEPALIVE_BYTES = 64 * 1024;
    const safelistedContentType = value => {
        if (value.length > 128 || /[\x00-\x08\x0a-\x1f\x7f"():<>?@\[\\\]{}]/.test(value)) return false;
        const essence = value.split(';', 1)[0].trim().toLowerCase();
        return essence === 'application/x-www-form-urlencoded' ||
            essence === 'multipart/form-data' || essence === 'text/plain';
    };
    Object.defineProperty(navigator, 'sendBeacon', {
        configurable: true, enumerable: true, writable: true,
        value(url, body = null) {
            if (arguments.length < 1) throw new TypeError('sendBeacon requires a URL');
            url = urlApi.resolve(urlApi.usv(url));
            if (!/^https?:\/\//i.test(url) || /^[a-z][a-z0-9+.-]*:\/\/[^/?#]*@/i.test(url))
                throw new TypeError('sendBeacon requires an HTTP(S) URL without credentials');
            const extracted = data.extractBody(body);
            if (extracted.stream) throw new TypeError('Beacon bodies cannot be streams');
            const bytes = extracted.bytes;
            if (bytes && bytes.byteLength > MAX_KEEPALIVE_BYTES) return false;
            const contentType = extracted.type;
            return !!__hostCall('beaconStart', JSON.stringify({
                url, method: 'POST', headers: contentType ? [['content-type', contentType]] : [],
                bodyBase64: bytes === null ? null : data.bytesToBase64(bytes),
                mode: !contentType || safelistedContentType(contentType) ? 'no-cors' : 'cors',
                credentials: 'include', cache: 'default', redirect: 'follow',
                referrer: 'about:client', referrerPolicy: 'strict-origin-when-cross-origin'
            }));
        }
    });
})();
