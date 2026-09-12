(() => {
    'use strict';
    const blobFromOwnedBytes = Blob.__fromOwnedBytes;
    const concatBytes = chunks => {
        const length = chunks.reduce((sum, chunk) => sum + chunk.length, 0);
        const bytes = new Uint8Array(length);
        let offset = 0;
        for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
        return bytes;
    };

    globalThis.__receiveXhrResponse = async (xhr, owner, response, progress, XMLHttpRequest) => {
        const current = () => xhr.__isCurrent(owner);
        if (!current()) return;
        xhr.__finishUpload('load', xhr.__uploadTotal, xhr.__uploadTotal);
        if (!current()) return;
        xhr.__status = response.status; xhr.__statusText = response.statusText;
        xhr.__responseURL = String(response.url).split('#', 1)[0]; xhr.__responseHeaders = response.headers;
        xhr.__changeState(XMLHttpRequest.HEADERS_RECEIVED);
        if (!current()) return;
        const chunks = [];
        let loaded = 0;
        let lastProgressLoaded = 0;
        let lastProgressAt = -Infinity;
        const total = Number(xhr.__responseHeaders.get('content-length')) || 0;
        const reader = response.body?.getReader() || null;
        const binaryResponse = xhr.__responseType === 'arraybuffer' || xhr.__responseType === 'blob';
        const textResponse = !binaryResponse;
        const decoder = new TextDecoder();
        try {
            if (reader) {
                while (current()) {
                    const { value, done } = await reader.read();
                    // Reopening can happen during the await, or synchronously from any
                    // author event below. No old body, error, or completion may escape.
                    if (!current()) return;
                    if (done) break;
                    if (!(value instanceof Uint8Array)) throw new TypeError('XHR body chunks must be bytes');
                    // Fetch owns each chunk; retaining its view avoids another full copy.
                    if (binaryResponse) chunks.push(value);
                    loaded += value.length;
                    if (textResponse) xhr.__appendResponseText(decoder.decode(value, { stream: true }));
                    const now = performance.now();
                    if (now - lastProgressAt >= 50) {
                        // LOADING begins with bytes, and readystatechange repeats with
                        // incremental progress for legacy web compatibility.
                        xhr.__changeState(XMLHttpRequest.LOADING);
                        if (!current()) return;
                        xhr.dispatchEvent(progress('progress', loaded, total));
                        if (!current()) return;
                        lastProgressLoaded = loaded; lastProgressAt = now;
                    }
                }
            }
        } catch (error) {
            if (current()) xhr.__requestError(error?.name === 'AbortError' ? 'abort' : 'error');
            return;
        } finally {
            reader?.releaseLock();
        }
        if (!current()) return;
        // End-of-body exposes the final byte count even within the 50 ms interval.
        if (loaded !== lastProgressLoaded) {
            xhr.dispatchEvent(progress('progress', loaded, total));
            if (!current()) return;
        }
        const contentType = xhr.__mime || xhr.__responseHeaders.get('content-type') || '';
        if (textResponse) xhr.__appendResponseText(decoder.decode());
        if (xhr.__responseType === 'arraybuffer') {
            xhr.__response = concatBytes(chunks).buffer;
        } else if (xhr.__responseType === 'blob') {
            xhr.__response = blobFromOwnedBytes(chunks, contentType);
        } else if (xhr.__responseType === 'json') {
            try { xhr.__response = JSON.parse(xhr.__materializeResponseText()); }
            catch (_) { xhr.__response = null; }
        } else if (xhr.__responseType === 'document') {
            const parser = typeof DOMParser === 'function' ? new DOMParser() : null;
            try { xhr.__response = xhr.__responseXML = parser?.parseFromString(xhr.__materializeResponseText(), contentType) || null; }
            catch (_) { xhr.__response = xhr.__responseXML = null; }
        }
        xhr.__finishSuccess(loaded, total);
    };
})();
