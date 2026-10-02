    // CSS animations own effects on the existing document timeline, not rewritten author styles.
    // Styles are rediscovered only after authored style/DOM changes, never after a sampled overlay.
    const cssAnimationToken = {};
    const cssAnimationRecords = new WeakMap();
    const cssAnimationTargets = new Map();
    let cssAnimationRevision = -1;
    let cssAnimationSyncing = false;
    const cssAnimationEventQueue = [];
    let cssAnimationEventFrame = null;

    class AnimationEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            init = init == null ? {} : Object(init);
            const elapsedTime = Number(init.elapsedTime ?? 0);
            if (!Number.isFinite(elapsedTime)) throw new TypeError('elapsedTime must be finite');
            Object.defineProperties(this, {
                animationName: {value:String(init.animationName ?? ''), enumerable:true},
                elapsedTime: {value:elapsedTime, enumerable:true},
                pseudoElement: {value:String(init.pseudoElement ?? ''), enumerable:true}
            });
        }
    }
    const queueCssAnimationEvent = (animation, type, elapsed) => {
        const record = cssAnimationRecords.get(animation);
        if (!record || cssAnimationEventQueue.length >= 2048) return;
        cssAnimationEventQueue.push([record.target, type, record.name, elapsed / 1000]);
        if (cssAnimationEventFrame !== null) return;
        cssAnimationEventFrame = windowObject.requestAnimationFrame(() => {
            cssAnimationEventFrame = null;
            const events = cssAnimationEventQueue.splice(0);
            for (const [target,type,name,elapsedTime] of events)
                target.dispatchEvent(new AnimationEvent(type,
                    {bubbles:true, animationName:name, elapsedTime}));
        });
    };
    const cssAnimationActiveTime = animation => {
        const timing = animation.effect?.__timing;
        if (!timing || animation.currentTime === null) return 0;
        return Math.max(0, Math.min(animationActiveDuration(timing), animation.currentTime - timing.delay));
    };
    const sampleCssAnimationEvents = animation => {
        const record = cssAnimationRecords.get(animation);
        const timing = animation.effect?.__timing;
        if (!record || !timing || animation.currentTime === null) return;
        const time = animation.currentTime;
        const end = timing.delay + animationActiveDuration(timing);
        const phase = time < timing.delay ? 'before' : time < end ? 'active' : 'after';
        if (phase !== 'before' && record.phase === 'before')
            queueCssAnimationEvent(animation, 'animationstart', Math.min(animationActiveDuration(timing), Math.max(0, -timing.delay)));
        if (phase === 'active' && timing.duration > 0) {
            const iteration = Math.floor(cssAnimationActiveTime(animation) / timing.duration);
            // Large clock advances do not create an unbounded queue of skipped iterations.
            if (record.phase === 'active' && iteration !== record.iteration)
                queueCssAnimationEvent(animation, 'animationiteration', iteration * timing.duration);
            record.iteration = iteration;
        }
        if (phase === 'after' && record.phase !== 'after')
            queueCssAnimationEvent(animation, 'animationend', animationActiveDuration(timing));
        record.phase = phase;
    };
    class CSSAnimation extends Animation {
        constructor(effect, record, token) {
            if (token !== cssAnimationToken) throw new TypeError('Illegal constructor');
            super(effect, document.timeline);
            cssAnimationRecords.set(this, record);
        }
        get animationName() { return cssAnimationRecords.get(this)?.name ?? ''; }
        get currentTime() { return super.currentTime; }
        set currentTime(value) { super.currentTime = value; sampleCssAnimationEvents(this); }
        __tick() { super.__tick(); sampleCssAnimationEvents(this); }
        cancel() {
            const record = cssAnimationRecords.get(this);
            if (record && this.playState !== 'idle' && record.phase !== 'after')
                queueCssAnimationEvent(this, 'animationcancel', cssAnimationActiveTime(this));
            if (record) record.phase = 'idle';
            super.cancel();
        }
        finish() { super.finish(); sampleCssAnimationEvents(this); }
    }
    const cssAnimationFrames = (blocks, easing) => blocks.map(([offset, declarations]) => {
        const frame = {offset, easing};
        for (const [name, value] of declarations) {
            if (name === 'animation-timing-function') frame.easing = value;
            else if (!name.startsWith('animation')) frame[name] = value;
        }
        return frame;
    });
    const updateCssAnimation = (animation, input, index) => {
        const record = cssAnimationRecords.get(animation);
        const [name,duration,delay,easing,iterations,direction,fill,state,blocks] = input;
        record.index = index;
        const signature = JSON.stringify(input, (_key,value) => value === Infinity ? 'infinite' : value);
        if (record.signature === signature) return;
        const previousState = record.cssState;
        record.signature = signature;
        record.cssState = state;
        // Native discovery already bounds the whole snapshot; keep the larger stylesheet
        // limits without weakening the public setKeyframes() resource contract.
        animation.effect.__frames = normalizeAnimationFrames(cssAnimationFrames(blocks, easing),
            {frames:256, properties:64});
        animation.__refresh();
        animation.effect.updateTiming({duration,delay,iterations,direction,fill,easing:'linear'});
        const properties = [...animationTargetProperties(animation)];
        const values = host('cssAnimationUnderlying', nodeId(record.target), properties);
        animation.__underlying = new Map(properties.map((property,index) => [property,values[index]]));
        if (previousState !== state) {
            if (state === 'paused') animation.pause();
            else animation.play();
        }
        sampleCssAnimationEvents(animation);
    };
    const cancelCssAnimation = animation => {
        animation.cancel();
    };
    syncCssAnimations = () => {
        if (cssAnimationSyncing || cssAnimationRevision === host('cssAnimationRevision')) return;
        cssAnimationSyncing = true;
        try {
            const seen = new Set();
            for (const [id, inputs] of host('cssAnimationSnapshot')) {
                const target = wrap(id);
                seen.add(target);
                const old = (cssAnimationTargets.get(target) ?? []).slice();
                const next = new Array(inputs.length);
                // Last-to-first matching preserves existing time when duplicate names reorder.
                for (let index = inputs.length - 1; index >= 0; index--) {
                    const input = inputs[index];
                    const match = old.findLastIndex(animation => animation.animationName === input[0]);
                    let animation;
                    if (match >= 0) [animation] = old.splice(match, 1);
                    else {
                        const record = {target, name:input[0], index, phase:'before', iteration:0,
                            cssState:null, signature:null};
                        animation = new CSSAnimation(new KeyframeEffect(target, [], 0), record, cssAnimationToken);
                    }
                    updateCssAnimation(animation, input, index);
                    next[index] = animation;
                }
                for (const animation of old) cancelCssAnimation(animation);
                cssAnimationTargets.set(target, next);
                applyAnimationStyles(target);
            }
            for (const [target, animations] of cssAnimationTargets)
                if (!seen.has(target)) {
                    for (const animation of animations) cancelCssAnimation(animation);
                    cssAnimationTargets.delete(target);
                }
            // Authored changes can alter implicit endpoints even when animation settings match.
            for (const [target, animations] of cssAnimationTargets) {
                for (const animation of animations) {
                    const properties = [...animationTargetProperties(animation)];
                    const values = host('cssAnimationUnderlying', nodeId(target), properties);
                    animation.__underlying = new Map(properties.map((property,index) => [property,values[index]]));
                }
                applyAnimationStyles(target);
            }
            cssAnimationRevision = host('cssAnimationRevision');
        } finally { cssAnimationSyncing = false; }
    };
    // Native checkpoints use the same private algorithm as author-facing style/animation queries.
    Object.defineProperty(windowObject, '__syncCssAnimations', {value:() => syncCssAnimations()});
    for (const constructor of [AnimationEvent, CSSAnimation])
        Object.defineProperty(constructor.prototype, Symbol.toStringTag,
            {value:constructor.name, configurable:true});
    Object.assign(windowObject, {AnimationEvent, CSSAnimation});
