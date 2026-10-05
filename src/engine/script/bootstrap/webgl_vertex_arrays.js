    const webGlVertexArrayExtensions = webGlPrivateBrands();
    class WebGLVertexArrayObjectOES {
        constructor(token) { if (token !== webGlToken) throw new TypeError('Illegal constructor'); }
    }
    Object.defineProperty(WebGLVertexArrayObjectOES.prototype, Symbol.toStringTag, {value:'WebGLVertexArrayObjectOES'});
    webGlObjectClasses.WebGLVertexArrayObjectOES = WebGLVertexArrayObjectOES;
    class OES_vertex_array_object {
        constructor(token, context) {
            if (token !== webGlToken) throw new TypeError('Illegal constructor');
            webGlVertexArrayExtensions.set(this, {context, epoch:webGlState(context).epoch});
        }
        createVertexArrayOES() {
            const context = webGlExtensionReceiver(this, webGlVertexArrayExtensions, 0, arguments, 'createVertexArrayOES');
            return context ? webGlObject(context, 'WebGLVertexArrayObjectOES', webGlCall(context, 'createVertexArrayOES')) : null;
        }
        deleteVertexArrayOES(arrayObject) {
            const context = webGlExtensionReceiver(this, webGlVertexArrayExtensions, 1, arguments, 'deleteVertexArrayOES');
            const converted = [arrayObject];
            webGlConvertInterface(converted, 0, 'WebGLVertexArrayObjectOES', true);
            arrayObject = converted[0];
            if (!context || arrayObject === null) return;
            const id = webGlHandle(context, arrayObject, 'WebGLVertexArrayObjectOES');
            if (id < 0) return;
            const object = webGlObjects.get(arrayObject);
            if (object.deleted) return;
            webGlCall(context, 'deleteVertexArrayOES', [id]);
            object.deleted = true;
        }
        isVertexArrayOES(arrayObject) {
            const context = webGlExtensionReceiver(this, webGlVertexArrayExtensions, 1, arguments, 'isVertexArrayOES');
            const converted = [arrayObject];
            webGlConvertInterface(converted, 0, 'WebGLVertexArrayObjectOES', true);
            arrayObject = converted[0];
            if (!context || arrayObject === null) return false;
            const object = webGlObjects.get(arrayObject);
            if (!object || object.type !== 'WebGLVertexArrayObjectOES') throw new TypeError('Expected WebGLVertexArrayObjectOES');
            return object.context === context && object.epoch === webGlState(context).epoch && !object.deleted &&
                Boolean(webGlCall(context, 'isVertexArrayOES', [object.id]));
        }
        bindVertexArrayOES(arrayObject) {
            const context = webGlExtensionReceiver(this, webGlVertexArrayExtensions, 1, arguments, 'bindVertexArrayOES');
            const converted = [arrayObject];
            webGlConvertInterface(converted, 0, 'WebGLVertexArrayObjectOES', true);
            arrayObject = converted[0];
            if (!context) return;
            const id = webGlHandle(context, arrayObject, 'WebGLVertexArrayObjectOES', true);
            if (id >= 0) {
                if (arrayObject !== null && webGlObjects.get(arrayObject).deleted) { webGlError(context, 0x0502); return; }
                webGlCall(context, 'bindVertexArrayOES', [id]);
            }
        }
    }
    Object.defineProperties(OES_vertex_array_object.prototype, {
        [Symbol.toStringTag]:{value:'OES_vertex_array_object'},
        VERTEX_ARRAY_BINDING_OES:{value:0x85b5, enumerable:true}
    });
    webGlExtensionFactories.set('oes_vertex_array_object', {
        name:'OES_vertex_array_object', create:context => new OES_vertex_array_object(webGlToken, context)
    });
