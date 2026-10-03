    const webGlDrawBuffersExtensions = new WeakMap();
    class WEBGL_draw_buffers {
        constructor(token, context) {
            if (token !== webGlToken) throw new TypeError('Illegal constructor');
            webGlDrawBuffersExtensions.set(this, {context, epoch:webGlState(context).epoch});
        }
        drawBuffersWEBGL(buffers) {
            const context = webGlExtensionReceiver(this, webGlDrawBuffersExtensions, 1, arguments, 'drawBuffersWEBGL');
            // Web IDL sequence conversion occurs before context-loss handling.
            const method = buffers == null ? undefined : buffers[Symbol.iterator];
            if (typeof method !== 'function') throw new TypeError('Expected an iterable buffer sequence');
            const values = [];
            const iterator = Reflect.apply(method, buffers, []);
            for (const value of {[Symbol.iterator]:() => iterator}) {
                if (values.length >= 8192) throw new RangeError('WebGL buffer sequence exceeds the command budget');
                values.push(webGlScalar('u', value));
            }
            if (context) webGlCall(context, 'drawBuffersWEBGL', values);
        }
    }
    Object.defineProperties(WEBGL_draw_buffers.prototype, {
        [Symbol.toStringTag]:{value:'WEBGL_draw_buffers'},
        MAX_COLOR_ATTACHMENTS_WEBGL:{value:0x8cdf, enumerable:true},
        MAX_DRAW_BUFFERS_WEBGL:{value:0x8824, enumerable:true}
    });
    for (let index = 0; index < 16; index++) {
        Object.defineProperty(WEBGL_draw_buffers.prototype, 'COLOR_ATTACHMENT' + index + '_WEBGL', {value:0x8ce0 + index, enumerable:true});
        Object.defineProperty(WEBGL_draw_buffers.prototype, 'DRAW_BUFFER' + index + '_WEBGL', {value:0x8825 + index, enumerable:true});
    }
    webGlExtensionFactories.set('webgl_draw_buffers', {
        name:'WEBGL_draw_buffers', create:context => new WEBGL_draw_buffers(webGlToken, context)
    });
