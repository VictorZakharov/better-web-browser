(() => {
    'use strict';
    const host = (...args) => __hostCall(...args);
    const pending = new Map();
    let failureDiagnostics = 0;
    const reportFailure = (id, detail) => {
        // Handled rejections still need diagnostic evidence. Never include request URLs,
        // response bodies, or server error text, which may contain credentials.
        if (failureDiagnostics++ < 32) {
            const origin = new URL(pending.get(Number(id)).request.url).origin;
            host('console', 'warn', 'Fetch ' + id + ' failed: ' + detail + ' origin=' + origin);
        }
    };

    globalThis.fetch = function fetch(input, init = undefined) {
        let request;
        try {
            request = new Request(input, init);
            request.signal.throwIfAborted();
        } catch (error) {
            return Promise.reject(error);
        }

        return new Promise((resolve, reject) => {
            const operation = {
                id: null, request, resolve, reject, responseStarted: false,
                completed: false, controller: null
            };
            operation.abort = () => {
                if (operation.completed) return;
                operation.completed = true;
                if (operation.id !== null) {
                    pending.delete(operation.id);
                    host('fetchAbort', operation.id);
                }
                if (operation.responseStarted) operation.controller?.error(request.signal.reason);
                else reject(request.signal.reason);
            };
            request.signal.addEventListener('abort', operation.abort, { once: true });
            request.__serialize().then(serialized => {
                if (operation.completed) return;
                try {
                    operation.id = Number(host('fetchStart', JSON.stringify(serialized)));
                    pending.set(operation.id, operation);
                } catch (error) {
                    operation.completed = true;
                    request.signal.removeEventListener('abort', operation.abort);
                    reject(error);
                }
            }, error => {
                if (operation.completed) return;
                operation.completed = true;
                request.signal.removeEventListener('abort', operation.abort);
                reject(error);
            });
        });
    };

    const finish = operation => {
        operation.completed = true;
        pending.delete(operation.id);
        operation.request.signal.removeEventListener('abort', operation.abort);
    };

    globalThis.__startFetch = (id, serialized) => {
        const operation = pending.get(Number(id));
        if (!operation || operation.completed || operation.responseStarted) return;
        const metadata = JSON.parse(String(serialized));
        if (metadata.errorName) {
            reportFailure(id, metadata.errorName === 'AbortError' ? 'aborted' : 'network error');
            finish(operation);
            operation.reject(metadata.errorName === 'AbortError'
                ? new DOMException(metadata.errorMessage, 'AbortError')
                : new TypeError(metadata.errorMessage));
            return;
        }
        const nullBody = operation.request.method === 'HEAD' ||
            [101, 204, 205, 304].includes(metadata.status) ||
            ['opaque', 'opaqueredirect', 'error'].includes(metadata.responseType);
        if (metadata.status >= 400) reportFailure(id, 'HTTP ' + metadata.status);
        let stream = null;
        if (!nullBody) {
            stream = new ReadableStream({
                start(controller) { operation.controller = controller; },
                cancel() {
                    if (!operation.completed) {
                        finish(operation);
                        host('fetchAbort', operation.id);
                    }
                }
            });
        }
        operation.responseStarted = true;
        operation.resolve(Response.__fromNetwork(metadata, stream, nullBody));
    };

    globalThis.__pushFetch = (id, body) => {
        const operation = pending.get(Number(id));
        if (!operation || operation.completed || !operation.responseStarted || !operation.controller) return;
        // The host creates a fresh Uint8Array for every IPC chunk. The stream owns that
        // value after enqueue, so copying it here only doubles large-response allocation.
        operation.controller.enqueue(body);
    };

    globalThis.__finishFetch = id => {
        const operation = pending.get(Number(id));
        if (!operation || operation.completed) return;
        operation.controller?.close();
        finish(operation);
    };

    globalThis.__abortFetch = (id, name, message) => {
        const operation = pending.get(Number(id));
        if (!operation || operation.completed) return;
        const error = String(name) === 'AbortError'
            ? new DOMException(String(message), 'AbortError') : new TypeError(String(message));
        reportFailure(id, String(name) === 'AbortError' ? 'aborted' : 'body stream error');
        if (operation.responseStarted) operation.controller?.error(error);
        else operation.reject(error);
        finish(operation);
    };
})();
