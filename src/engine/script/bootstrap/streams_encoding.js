(() => {
    'use strict';

    // Encoding Standard: an unpaired leading surrogate is buffered across writes.
    // Encoding each input chunk independently would corrupt a split astral character.
    class TextEncoderStream {
        constructor() {
            const encoder = new TextEncoder();
            let leading = '';
            const transform = new TransformStream({
                transform(chunk, controller) {
                    let input = leading + (chunk === undefined ? 'undefined' : '' + chunk);
                    leading = '';
                    if (input.length) {
                        const last = input.charCodeAt(input.length - 1);
                        if (last >= 0xD800 && last <= 0xDBFF) {
                            leading = input.slice(-1);
                            input = input.slice(0, -1);
                        }
                    }
                    if (input.length) controller.enqueue(encoder.encode(input));
                },
                flush(controller) {
                    if (leading) controller.enqueue(encoder.encode('\uFFFD'));
                }
            });
            this.readable = transform.readable;
            this.writable = transform.writable;
        }
        get encoding() { return 'utf-8'; }
    }

    class TextDecoderStream {
        constructor(label = 'utf-8', options = {}) {
            const decoder = new TextDecoder(label, options);
            const transform = new TransformStream({
                transform(chunk, controller) {
                    const output = decoder.decode(chunk, { stream: true });
                    if (output) controller.enqueue(output);
                },
                flush(controller) {
                    const output = decoder.decode();
                    if (output) controller.enqueue(output);
                }
            });
            this.readable = transform.readable;
            this.writable = transform.writable;
            this.__decoder = decoder;
        }
        get encoding() { return this.__decoder.encoding; }
        get fatal() { return this.__decoder.fatal; }
        get ignoreBOM() { return this.__decoder.ignoreBOM; }
    }

    Object.assign(globalThis, { TextEncoderStream, TextDecoderStream });
})();
