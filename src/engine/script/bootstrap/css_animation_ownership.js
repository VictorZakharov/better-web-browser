    // CSS Animations 2 §2 extends the underlying Animation algorithms, not
    // only CSSAnimation overrides. Calling Animation.prototype.play.call(css)
    // or its base effect/startTime setters must have identical ownership rules.
    // Install this policy once during private bootstrap, before any author code.
    const applyCssAnimationOperation = Reflect.apply;
    const readCssAnimationPlayState = Object.getOwnPropertyDescriptor(Animation.prototype, 'playState').get;
    const runCssAnimationControl = (animation, kind, operation, arguments_) => {
        const record = cssAnimationRecords.get(animation);
        if (!record || cssAnimationSyncing)
            return applyCssAnimationOperation(operation, animation, arguments_);
        const before = applyCssAnimationOperation(readCssAnimationPlayState, animation, []);
        const outer = record.controlDepth++ === 0;
        try {
            const result = applyCssAnimationOperation(operation, animation, arguments_);
            const after = applyCssAnimationOperation(readCssAnimationPlayState, animation, []);
            // Nested play/pause calls from reverse/startTime do not independently
            // claim CSS state. Only the successful outer operation decides.
            if (outer && (kind === 'play' || kind === 'pause' ||
                before !== after && (before === 'paused' || after === 'paused')))
                record.playStateOverridden = true;
            return result;
        } finally { record.controlDepth--; }
    };
    for (const kind of ['play', 'pause', 'reverse']) {
        const descriptor = Object.getOwnPropertyDescriptor(Animation.prototype, kind);
        const method = function () { return runCssAnimationControl(this, kind, descriptor.value, []); };
        Object.defineProperty(method, 'name', {value:descriptor.value.name, configurable:true});
        Object.defineProperty(Animation.prototype, kind, {...descriptor, value:method});
    }
    {
        const descriptor = Object.getOwnPropertyDescriptor(Animation.prototype, 'startTime');
        const setter = function (value) { runCssAnimationControl(this, 'startTime', descriptor.set, [value]); };
        Object.defineProperty(setter, 'name', {value:descriptor.set.name, configurable:true});
        Object.defineProperty(Animation.prototype, 'startTime', {...descriptor, set:setter});
    }
    {
        const descriptor = Object.getOwnPropertyDescriptor(Animation.prototype, 'effect');
        const setter = function (value) {
            applyCssAnimationOperation(descriptor.set, this, [value]);
            const record = cssAnimationRecords.get(this);
            if (record && value !== record.originalEffect) record.effectReplaced = true;
        };
        Object.defineProperty(setter, 'name', {value:descriptor.set.name, configurable:true});
        Object.defineProperty(Animation.prototype, 'effect', {...descriptor, set:setter});
    }
