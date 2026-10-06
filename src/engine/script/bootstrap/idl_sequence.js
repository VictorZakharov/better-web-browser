    // Web IDL conversion failures propagate without IteratorClose, unlike
    // Array.from/spread. https://webidl.spec.whatwg.org/#create-sequence-from-iterable
    const idlSequenceApply = Reflect.apply;
    const idlSequenceDefine = Object.defineProperty;
    const idlSequenceObject = value => value !== null &&
        (typeof value === 'object' || typeof value === 'function');
    const idlSequenceFromIterator = (value, method, convert) => {
        const iterator = idlSequenceApply(method, value, []);
        if (!idlSequenceObject(iterator)) throw new TypeError('Invalid sequence iterator');
        const next = iterator.next, result = [];
        for (;;) {
            const step = idlSequenceApply(next, iterator, []);
            if (!idlSequenceObject(step)) throw new TypeError('Invalid sequence iterator result');
            if (step.done) return result;
            idlSequenceDefine(result, result.length, {value:convert(step.value),
                enumerable:true, configurable:true, writable:true});
        }
    };
    const idlSequence = (value, convert) => {
        if (!idlSequenceObject(value)) throw new TypeError('Expected an iterable object');
        const method = value[Symbol.iterator];
        if (typeof method !== 'function') throw new TypeError('Sequence is not iterable');
        return idlSequenceFromIterator(value, method, convert);
    };
