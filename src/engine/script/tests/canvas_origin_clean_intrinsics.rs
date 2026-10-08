//! Public prototype hooks must never receive private, possibly tainted storage.
use super::canvas_origin_clean::check;

#[test]
fn canvas_origin_clean_weakmap_hooks_cannot_observe_bitmap_records() {
    check(
        r#"
        const source = document.querySelector('canvas'), ctx = source.getContext('2d');
        ctx.filter = 'drop-shadow(1px 0 currentColor)'; ctx.fillRect(0, 0, 2, 2);
        const get = WeakMap.prototype.get, set = WeakMap.prototype.set;
        let observed = 0;
        WeakMap.prototype.get = function(key) {
            const value = get.call(this, key);
            if (value?.pixels) observed++;
            return value;
        };
        WeakMap.prototype.set = function(key, value) {
            if (value?.pixels) observed++;
            return set.call(this, key, value);
        };
        createImageBitmap(source, 0, 0, 2, 2, {resizeWidth:4, imageOrientation:'flipY'}).then(bitmap => {
            const target = document.createElement('canvas'); target.getContext('2d').drawImage(bitmap, 0, 0);
            let denied = false;
            try {target.toDataURL();}catch(error){denied = error.name === 'SecurityError';}
            WeakMap.prototype.get = get; WeakMap.prototype.set = set;
            document.querySelector('output').textContent = observed === 0 && denied ? 'yes' : observed + ':' + denied;
        }, error => {
            WeakMap.prototype.get = get; WeakMap.prototype.set = set;
            throw error;
        });
    "#,
    );
}

#[test]
fn canvas_origin_clean_typed_array_hooks_cannot_observe_private_copy_or_samples() {
    check(
        r#"
        const source = document.querySelector('canvas'), ctx = source.getContext('2d');
        ctx.filter = 'drop-shadow(1px 0 currentColor)'; ctx.fillRect(0, 0, 2, 2);
        const Constructor = Uint8ClampedArray, proto = Object.getPrototypeOf(Constructor.prototype);
        const set = proto.set, subarray = proto.subarray;
        const properties = ['length', 'byteLength', 'byteOffset', 'buffer'];
        const descriptors = properties.map(name => Object.getOwnPropertyDescriptor(proto, name));
        let observed = 0;
        globalThis.Uint8ClampedArray = function(...args) {observed++;return new Constructor(...args);};
        proto.set = function(...args) {observed++;return set.apply(this, args);};
        proto.subarray = function(...args) {observed++;return subarray.apply(this, args);};
        for (let index = 0; index < properties.length; index++) Object.defineProperty(proto, properties[index], {
            configurable:true, get(){observed++; return descriptors[index].get.call(this);}
        });
        const restore = () => {
            globalThis.Uint8ClampedArray = Constructor; proto.set = set; proto.subarray = subarray;
            properties.forEach((name, index) => Object.defineProperty(proto, name, descriptors[index]));
        };
        createImageBitmap(source, 0, 0, 2, 2, {resizeWidth:4, imageOrientation:'flipY', premultiplyAlpha:'premultiply'}).then(bitmap => {
            const target = document.createElement('canvas'), drawing = target.getContext('2d');
            drawing.imageSmoothingEnabled = false; drawing.drawImage(bitmap, 0, 0);
            restore();
            let denied = false;
            try {target.toDataURL();}catch(error){denied = error.name === 'SecurityError';}
            document.querySelector('output').textContent = observed === 0 && denied ? 'yes' : observed + ':' + denied;
        }, error => {restore();throw error;});
    "#,
    );
}

#[test]
fn canvas_origin_clean_filter_array_hooks_cannot_relabel_private_operations() {
    check(
        r#"
        const source = document.querySelector('canvas'), ctx = source.getContext('2d');
        const some = Array.prototype.some, iterator = Array.prototype[Symbol.iterator];
        let observed = 0;
        const intercept = value => {
            if (value?.[0]?.name === 'drop-shadow') {
                observed++; value[0].value.originClean = true;
            }
        };
        Array.prototype.some = function(...args){intercept(this);return some.apply(this,args);};
        Array.prototype[Symbol.iterator] = function(){intercept(this);return iterator.call(this);};
        try {
            ctx.filter = 'drop-shadow(1px 0 currentColor) brightness(.5)'; ctx.fillRect(0, 0, 2, 2);
        } finally {Array.prototype.some = some; Array.prototype[Symbol.iterator] = iterator;}
        let denied = false;
        try {source.toDataURL();}catch(error){denied = error.name === 'SecurityError';}
        document.querySelector('output').textContent = observed === 0 && denied ? 'yes' : observed + ':' + denied;
    "#,
    );
}

#[test]
fn canvas_origin_clean_saved_state_is_not_exposed_through_array_push_or_pop() {
    check(
        r#"
        const source = document.querySelector('canvas'), ctx = source.getContext('2d');
        ctx.filter = 'drop-shadow(1px 0 currentColor)';
        const push = Array.prototype.push, pop = Array.prototype.pop;
        let observed = 0;
        const inspect = value => {
            if (value?.filterOperations?.length) {
                observed++; value.filterOperations[0].value.originClean = true;
            }
        };
        Array.prototype.push = function(...args){inspect(args[0]);return push.apply(this,args);};
        Array.prototype.pop = function(){const value=pop.call(this);inspect(value);return value;};
        try {ctx.save();ctx.filter='none';ctx.restore();ctx.fillRect(0,0,2,2);}
        finally {Array.prototype.push=push;Array.prototype.pop=pop;}
        let denied = false;
        try {source.toDataURL();}catch(error){denied=error.name==='SecurityError';}
        document.querySelector('output').textContent = observed===0 && denied ? 'yes' : observed+':'+denied;
    "#,
    );
}

#[test]
fn canvas_origin_clean_arithmetic_hooks_do_not_observe_protected_samples() {
    check(
        r#"
        const source = document.querySelector('canvas'), ctx = source.getContext('2d');
        ctx.fillStyle = '#895b39'; ctx.filter='drop-shadow(1px 0 currentColor)';ctx.fillRect(0,0,2,2);
        const target = document.createElement('canvas'), drawing = target.getContext('2d');
        const names = ['round','floor','ceil','min','max','sqrt','abs','cos','sin'];
        const originals = names.map(name=>Math[name]);
        let observed = 0;
        for(let index=0;index<names.length;index++) Math[names[index]]=function(...args){
            observed++; return originals[index](...args);
        };
        try {
            drawing.filter='brightness(.5) saturate(1.2) hue-rotate(20deg)';
            for(const operator of ['source-over','multiply','hue','color']) {
                drawing.globalCompositeOperation=operator; drawing.drawImage(source,0,0);
            }
        } finally {names.forEach((name,index)=>Math[name]=originals[index]);}
        let denied=false;
        try{target.toDataURL();}catch(error){denied=error.name==='SecurityError';}
        document.querySelector('output').textContent=observed===0 && denied ? 'yes' : observed+':'+denied;
    "#,
    );
}
