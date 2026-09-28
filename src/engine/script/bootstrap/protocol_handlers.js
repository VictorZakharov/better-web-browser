// HTML custom-scheme handlers are browser-owned. This facade only validates and
// sends an intent; a website cannot select a default or claim that consent succeeded.
// https://html.spec.whatwg.org/multipage/system-state.html#custom-handlers
(() => {
    'use strict';
    const native = globalThis.__hostCall;
    if (!native('protocolHandlerAvailable')) return;
    const request = (action, scheme, url) => {
        if (typeof scheme === 'symbol' || typeof url === 'symbol')
            throw new TypeError('Protocol handler parameters must be strings');
        const result = String(native('protocolHandlerRequest', action, String(scheme), String(url).toWellFormed()));
        if (result) throw new DOMException(
            result === 'SyntaxError' ? 'Invalid protocol handler URL template' :
                'Protocol handler registration is not allowed', result);
    };
    Object.defineProperties(navigator, {
        registerProtocolHandler: {
            configurable: true, enumerable: true, writable: true,
            value: function(scheme, url) {
                if (arguments.length < 2) throw new TypeError('Two arguments are required');
                if (this !== navigator) throw new TypeError('Invalid Navigator receiver');
                return request('register', scheme, url);
            }
        },
        unregisterProtocolHandler: {
            configurable: true, enumerable: true, writable: true,
            value: function(scheme, url) {
                if (arguments.length < 2) throw new TypeError('Two arguments are required');
                if (this !== navigator) throw new TypeError('Invalid Navigator receiver');
                return request('unregister', scheme, url);
            }
        }
    });
})();
