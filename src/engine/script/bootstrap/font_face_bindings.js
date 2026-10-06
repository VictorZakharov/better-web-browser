    // Web IDL's (CSSOMString or BufferSource) union chooses actual internal
    // buffer/view brands, never instanceof or author-shadowed storage properties.
    const fontBytes = Uint8Array;
    const fontHost = __hostCall;
    const fontFetch = globalThis.__fontFaceFetch;
    delete globalThis.__fontFaceFetch;
    const FontPromise = Promise;
    const fontPromiseReject = Promise.reject.bind(Promise);
    const fontPromiseAll = Promise.all.bind(Promise);
    // Both realm bootstraps supply a private, non-cancelable font task source.
    // Consume the handoff before author scripts can replace the scheduler.
    const queueFontTask = globalThis.__fontLoadingQueue;
    delete globalThis.__fontLoadingQueue;
    const fontCreate = Object.create;
    const fontWeakGet = Function.call.bind(WeakMap.prototype.get);
    const fontWeakHas = Function.call.bind(WeakMap.prototype.has);
    const fontWeakSet = Function.call.bind(WeakMap.prototype.set);
    const fontEventDispatch = Function.call.bind(EventTarget.prototype.dispatchEvent);
    const fontTrustEvent = globalThis.__markTrustedEvent;
    const fontJSONParse = JSON.parse, fontJSONStringify = JSON.stringify;
    const fontIsView = ArrayBuffer.isView;
    const fontBufferLength = Function.call.bind(
        Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, 'byteLength').get);
    const fontBufferResizable = Function.call.bind(
        Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, 'resizable').get);
    const fontSharedLength = typeof SharedArrayBuffer === 'function' ?
        Function.call.bind(Object.getOwnPropertyDescriptor(SharedArrayBuffer.prototype, 'byteLength').get) : null;
    const fontTypedPrototype = Object.getPrototypeOf(Uint8Array.prototype);
    const fontViewBuffer = Function.call.bind(
        Object.getOwnPropertyDescriptor(fontTypedPrototype, 'buffer').get);
    const fontViewOffset = Function.call.bind(
        Object.getOwnPropertyDescriptor(fontTypedPrototype, 'byteOffset').get);
    const fontViewLength = Function.call.bind(
        Object.getOwnPropertyDescriptor(fontTypedPrototype, 'byteLength').get);
    const fontDataBuffer = Function.call.bind(
        Object.getOwnPropertyDescriptor(DataView.prototype, 'buffer').get);
    const fontDataOffset = Function.call.bind(
        Object.getOwnPropertyDescriptor(DataView.prototype, 'byteOffset').get);
    const fontDataLength = Function.call.bind(
        Object.getOwnPropertyDescriptor(DataView.prototype, 'byteLength').get);
    const fontBytesSet = Function.call.bind(Uint8Array.prototype.set);
    const fontString = value => `${value}`;
    const fontBufferSource = source => {
        let buffer, offset, length;
        if (fontIsView(source)) {
            try {
                buffer = fontViewBuffer(source);
                offset = fontViewOffset; length = fontViewLength;
            } catch {
                buffer = fontDataBuffer(source);
                offset = fontDataOffset; length = fontDataLength;
            }
        } else {
            try { fontBufferLength(source); buffer = source; }
            catch {
                let shared = false;
                if (fontSharedLength) try { fontSharedLength(source); shared = true; } catch {}
                if (shared) throw new TypeError('Shared font sources are unsupported');
                const input=fontCreate(null);input.string=fontString(source);return input;
            }
        }
        // ArrayBuffer's getter rejects shared memory; Web IDL also excludes
        // resizable buffers unless the parameter explicitly opts in.
        fontBufferLength(buffer);
        if (fontBufferResizable(buffer)) throw new TypeError('Resizable font sources are unsupported');
        const input=fontCreate(null);
        input.source=source;input.buffer=buffer;input.offset=offset;input.length=length;
        return input;
    };
    const fontCopySource = input => {
        let offset = 0, length = fontBufferLength(input.buffer);
        if (input.offset) {
            try { offset = input.offset(input.source); length = input.length(input.source); }
            catch { length = 0; } // A detached DataView contains no font bytes.
        }
        if (length > 32 * 1024 * 1024)
            throw new DOMException('Font source exceeds the byte budget', 'NotSupportedError');
        const copy = new fontBytes(length);
        if (length) fontBytesSet(copy, new fontBytes(input.buffer, offset, length));
        return copy;
    };
    const fontDescriptorDictionary = descriptors => {
        if (descriptors !== undefined && descriptors !== null &&
            typeof descriptors !== 'object' && typeof descriptors !== 'function')
            throw new TypeError('FontFace descriptors must be a dictionary');
        const values = fontCreate(null);
        // Dictionary members are read once, in lexicographic order.
        const names=['ascentOverride','descentOverride','display','featureSettings',
            'lineGapOverride','stretch','style','unicodeRange','variationSettings','weight'];
        for (let index=0;index<names.length;index++) {
            const name=names[index];
            const value = descriptors?.[name];
            values[name] = value === undefined ? fontDefaults[name] : fontString(value);
        }
        const width = descriptors?.width;
        if (width !== undefined) values.stretch = fontString(width);
        values.variant = fontDefaults.variant;
        return values;
    };
