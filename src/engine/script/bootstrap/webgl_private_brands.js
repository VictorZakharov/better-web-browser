    // Web IDL receiver checks use implementation slots, never author-overridable
    // WeakMap methods or constructors. Capture once before page script runs.
    const webGlNativeWeakMap = WeakMap;
    const webGlWeakGet = Function.prototype.call.bind(WeakMap.prototype.get);
    const webGlWeakSet = Function.prototype.call.bind(WeakMap.prototype.set);
    const webGlWeakHas = Function.prototype.call.bind(WeakMap.prototype.has);
    const webGlWeakDelete = Function.prototype.call.bind(WeakMap.prototype.delete);
    const webGlPrivateBrands = () => {
        const records = new webGlNativeWeakMap();
        return {
            get:key => webGlWeakGet(records,key),
            set:(key,value) => { webGlWeakSet(records,key,value); },
            has:key => webGlWeakHas(records,key),
            delete:key => webGlWeakDelete(records,key)
        };
    };
