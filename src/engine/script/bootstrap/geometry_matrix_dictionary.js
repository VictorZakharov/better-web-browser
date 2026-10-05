    // DOMMatrix2DInit is a dictionary, not the DOMMatrix constructor's
    // string/sequence overload. Read each member once in Web IDL name order.
    const matrixDictionary2D = source => {
        if(source===undefined||source===null)return identity2D();
        if(typeof source!=='object'&&typeof source!=='function')
            throw new TypeError('DOMMatrix2DInit requires a dictionary');
        const names=['a','b','c','d','e','f','m11','m12','m21','m22','m41','m42'];
        const values=[];
        for(let index=0;index<names.length;index++) {
            const value=source[names[index]];
            values[index]=value===undefined?undefined:+value;
        }
        const result=[];
        for(let index=0;index<6;index++) {
            const short=values[index],long=values[index+6];
            // Geometry Interfaces fixup compares aliases with SameValueZero:
            // NaN agrees with NaN and signed zero aliases also agree.
            if(short!==undefined&&long!==undefined&&short!==long&&
                !(Number.isNaN(short)&&Number.isNaN(long)))
                throw new TypeError('Conflicting matrix component aliases');
            result[index]=short??long??(index===0||index===3?1:0);
        }
        return result;
    };
