    // Web Animations Level 1: a document-timeline animation origin, separate
    // from author style declarations and observable style attributes.
    // https://www.w3.org/TR/web-animations-1/#dom-element-animate
    const documentAnimations = new Set();
    const animationAppliedStyles = new WeakMap();
    let animationFrameHandle = null;
    const animationNow = () => performance.now();
    const animationEndTime = timing => Math.max(0,
        timing.delay + animationActiveDuration(timing) + timing.endDelay);
    const applyAnimationStyles = target => {
        if (!target) return;
        const declarations = new Map();
        for (const animation of documentAnimations) {
            if (animation.effect?.target !== target || animation.replaceState === 'removed') continue;
            const progress = sampleAnimationProgress(animation.effect.__timing, animation.currentTime);
            for (const [property, value] of sampleAnimationValues(animation.effect,
                progress, animation.__underlying)) declarations.set(property, value);
        }
        const style = [...declarations].map(([name, value]) => `${name}: ${value};`).join(' ');
        if (animationAppliedStyles.get(target) === style) return;
        animationAppliedStyles.set(target, style);
        host('setAnimationStyle', nodeId(target), style);
    };
    const animationTargetProperties = animation => new Set(
        animation.effect?.__frames.flatMap(frame => [...frame.values.keys()]) ?? []);
    const replaceableAnimation = animation => animation.playState === 'finished' &&
        animation.timeline instanceof DocumentTimeline && animation.effect?.target?.isConnected &&
        sampleAnimationProgress(animation.effect.__timing, animation.currentTime) !== null &&
        animation.replaceState !== 'removed';
    const removeReplacedAnimations = () => {
        // A finished filling effect can leave the stack only when every one
        // of its properties is covered by a later replaceable effect.
        // https://www.w3.org/TR/web-animations-1/#remove-replaced-animations
        const animations = [...documentAnimations], changed = new Set();
        for (let index = 0; index < animations.length; index++) {
            const older = animations[index];
            if (!replaceableAnimation(older) || older.replaceState !== 'active') continue;
            const properties = animationTargetProperties(older);
            if (!properties.size) continue;
            const covered = new Set();
            for (const newer of animations.slice(index + 1)) {
                if (!replaceableAnimation(newer) || newer.effect.target !== older.effect.target) continue;
                for (const property of animationTargetProperties(newer)) covered.add(property);
            }
            if (![...properties].every(property => covered.has(property))) continue;
            older.__replaceState = 'removed';
            changed.add(older.effect.target);
            windowObject.queueMicrotask(() => older.__event('remove'));
        }
        for (const target of changed) applyAnimationStyles(target);
    };
    const scheduleAnimationTick = () => {
        if (animationFrameHandle !== null || ![...documentAnimations].some(animation =>
            animation.playState === 'running' && animation.playbackRate !== 0 &&
            animation.__clock() !== null)) return;
        animationFrameHandle = windowObject.requestAnimationFrame(() => {
            animationFrameHandle = null;
            for (const animation of [...documentAnimations]) animation.__tick();
            removeReplacedAnimations();
            scheduleAnimationTick();
        });
    };
    class AnimationPlaybackEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            Object.defineProperties(this, {
                currentTime: {value: init.currentTime ?? null, enumerable: true},
                timelineTime: {value: init.timelineTime ?? null, enumerable: true}
            });
        }
    }
    class AnimationTimeline {
        constructor() {
            if (new.target === AnimationTimeline) throw new TypeError('Illegal constructor');
        }
        get currentTime() { return null; }
    }
    class DocumentTimeline extends AnimationTimeline {
        constructor(options = {}) {
            super();
            this.originTime = Number(options?.originTime ?? 0);
            if (!Number.isFinite(this.originTime))
                throw new TypeError('DocumentTimeline originTime must be finite');
        }
        get currentTime() { return animationNow() - this.originTime; }
    }
    class Animation extends EventTarget {
        constructor(effect = null, timeline = document.timeline) {
            super();
            if (effect !== null && !(effect instanceof KeyframeEffect))
                throw new TypeError('Animation effect must be a KeyframeEffect or null');
            if (timeline !== null && !(timeline instanceof AnimationTimeline))
                throw new TypeError('Animation timeline must be an AnimationTimeline or null');
            this.__effect = null;
            this.__timeline = timeline;
            this.__holdTime = null;
            this.__anchor = null;
            this.__state = 'idle';
            this.__rate = 1;
            this.__underlying = new Map();
            this.__finishedPromise = null;
            this.__resolveFinished = null;
            this.__rejectFinished = null;
            this.__readyPromise = Promise.resolve(this);
            this.__resolveReady = null;
            this.__rejectReady = null;
            this.__pendingTask = null;
            this.__pendingVersion = 0;
            this.__replaceState = 'active';
            this.__id = '';
            this.onfinish = null;
            this.oncancel = null;
            this.onremove = null;
            this.effect = effect;
        }
        get effect() { return this.__effect; }
        get id() { return this.__id; }
        set id(value) { this.__id = String(value); }
        set effect(value) {
            if (value !== null && !(value instanceof KeyframeEffect))
                throw new TypeError('Animation effect must be a KeyframeEffect or null');
            // An effect belongs to at most one animation. Detaching the prior
            // owner also removes its contribution from the animation origin.
            if (value?.__animation && value.__animation !== this)
                value.__animation.effect = null;
            const previous = this.__effect?.target;
            if (this.__effect) this.__effect.__animation = null;
            this.__effect = value;
            if (value) {
                value.__animation = this;
                this.__underlying = animationUnderlyings(value);
            }
            applyAnimationStyles(previous);
            this.__refresh();
        }
        get timeline() { return this.__timeline; }
        set timeline(value) {
            if (value !== null && !(value instanceof AnimationTimeline))
                throw new TypeError('Animation timeline must be an AnimationTimeline or null');
            const current = this.currentTime;
            this.__timeline = value;
            this.__holdTime = current;
            this.__anchor = this.__clock();
            if (this.__pendingTask && this.__clock() !== null)
                this.__queuePending(this.__pendingTask);
            this.__refresh();
        }
        __clock() { return this.__timeline?.currentTime ?? null; }
        get playbackRate() { return this.__rate; }
        set playbackRate(value) {
            const current = this.currentTime;
            const rate = Number(value);
            if (!Number.isFinite(rate)) throw new TypeError('Playback rate must be finite');
            this.__rate = rate;
            this.__holdTime = current;
            this.__anchor = this.__clock();
            this.__refresh();
        }
        get currentTime() {
            if (this.__holdTime === null) return null;
            return this.__state === 'running' && this.__pendingTask !== 'play' &&
                this.__clock() !== null && this.__anchor !== null ?
                this.__holdTime + (this.__clock() - this.__anchor) * this.__rate : this.__holdTime;
        }
        set currentTime(value) {
            this.__holdTime = value == null ? null : Number(value);
            if (this.__holdTime !== null && !Number.isFinite(this.__holdTime))
                throw new TypeError('Animation currentTime must be finite');
            this.__anchor = this.__clock();
            this.__refresh();
        }
        get startTime() {
            return this.__state === 'running' && this.__pendingTask !== 'play' &&
                this.__clock() !== null && this.__anchor !== null && this.__rate ?
                this.__anchor - this.__holdTime / this.__rate : null;
        }
        set startTime(value) {
            if (value === null) { this.pause(); return; }
            const start = Number(value);
            if (!Number.isFinite(start)) throw new TypeError('Animation startTime must be finite');
            this.__clearPending();
            this.__holdTime = 0;
            this.__anchor = start;
            this.__state = 'running';
            documentAnimations.add(this);
            this.__refresh();
        }
        get playState() { return this.__state; }
        get pending() { return this.__pendingTask !== null; }
        get replaceState() { return this.__replaceState; }
        get ready() { return this.__readyPromise; }
        get finished() {
            if (!this.__finishedPromise) this.__finishedPromise = new Promise((resolve, reject) => {
                this.__resolveFinished = resolve;
                this.__rejectFinished = reject;
                if (this.__state === 'finished') resolve(this);
            });
            return this.__finishedPromise;
        }
        __queuePending(kind) {
            // A ready task is asynchronous even when no compositor setup is
            // required. Canceling it before this checkpoint must still reject
            // the observable ready promise (Web Animations §4.4.7–4.4.9).
            if (this.__pendingTask === null)
                this.__readyPromise = new Promise((resolve, reject) => {
                    this.__resolveReady = resolve;
                    this.__rejectReady = reject;
                });
            this.__pendingTask = kind;
            const version = ++this.__pendingVersion;
            windowObject.queueMicrotask(() => {
                if (version !== this.__pendingVersion || this.__pendingTask !== kind) return;
                if (this.__clock() === null) return;
                if (kind === 'play') this.__anchor = this.__clock();
                this.__pendingTask = null;
                this.__resolveReady?.(this);
                this.__resolveReady = this.__rejectReady = null;
                this.__refresh();
            });
        }
        __clearPending(reject = false) {
            if (this.__pendingTask === null) return;
            this.__pendingVersion++;
            this.__pendingTask = null;
            if (reject) this.__rejectReady?.(new DOMException('Animation canceled', 'AbortError'));
            else this.__resolveReady?.(this);
            this.__resolveReady = this.__rejectReady = null;
            if (reject) this.__readyPromise = Promise.resolve(this);
        }
        __event(type) {
            const event = new AnimationPlaybackEvent(type, {
                currentTime: this.currentTime, timelineTime: this.timeline?.currentTime ?? null
            });
            this.dispatchEvent(event);
            const handler = type === 'finish' ? this.onfinish :
                type === 'remove' ? this.onremove : this.oncancel;
            if (typeof handler === 'function') handler.call(this, event);
        }
        __refresh() {
            if (!this.effect?.target) return;
            if (this.__state !== 'idle') documentAnimations.add(this);
            applyAnimationStyles(this.effect.target);
            scheduleAnimationTick();
        }
        __retarget(previous) {
            this.__underlying = this.effect ? animationUnderlyings(this.effect) : new Map();
            applyAnimationStyles(previous);
            this.__refresh();
        }
        __rekeyframe() {
            if (!this.effect?.target) return;
            const next = animationUnderlyings(this.effect);
            for (const [property, value] of next) {
                if (!this.__underlying.has(property)) this.__underlying.set(property, value);
            }
            this.__refresh();
        }
        __tick() {
            if (this.__state !== 'running') return;
            const end = animationEndTime(this.effect?.__timing ?? normalizeAnimationTiming(0));
            const current = this.currentTime;
            if (this.__rate >= 0 && current >= end || this.__rate < 0 && current <= 0) {
                this.finish();
            } else applyAnimationStyles(this.effect?.target);
        }
        play() {
            if (this.__state === 'running' && this.__pendingTask !== 'pause') return;
            const end = animationEndTime(this.effect?.__timing ?? normalizeAnimationTiming(0));
            const current = this.currentTime;
            if (this.__rate < 0 && !Number.isFinite(end) && (current === null || current <= 0))
                throw new DOMException('Cannot reverse an infinite animation', 'InvalidStateError');
            const wasFinished = this.__state === 'finished';
            this.__holdTime = current === null || this.__rate >= 0 && current >= end ?
                (this.__rate < 0 ? end : 0) : current;
            this.__anchor = this.__clock();
            this.__state = 'running';
            if (this.__replaceState === 'removed') this.__replaceState = 'active';
            if (wasFinished) this.__finishedPromise = null;
            this.__queuePending('play');
            this.__refresh();
        }
        pause() {
            if (this.__state === 'paused') return;
            this.__holdTime = this.currentTime ?? 0;
            this.__state = 'paused';
            this.__queuePending('pause');
            this.__refresh();
        }
        reverse() {
            this.playbackRate = this.__rate === 0 ? -1 : -this.__rate;
            this.play();
        }
        finish() {
            const end = animationEndTime(this.effect?.__timing ?? normalizeAnimationTiming(0));
            if (!Number.isFinite(end) || this.__rate === 0)
                throw new DOMException('Cannot finish this animation', 'InvalidStateError');
            if (this.__state === 'finished') return;
            this.__clearPending();
            this.__holdTime = this.__rate < 0 ? 0 : end;
            this.__state = 'finished';
            this.__refresh();
            this.__resolveFinished?.(this);
            if (!this.effect || sampleAnimationProgress(
                this.effect.__timing, this.__holdTime) === null)
                documentAnimations.delete(this);
            windowObject.queueMicrotask(() => this.__event('finish'));
        }
        cancel() {
            if (this.__state === 'idle') return;
            const target = this.effect?.target;
            const wasFinished = this.__state === 'finished';
            this.__clearPending(true);
            this.__readyPromise = Promise.resolve(this);
            this.__holdTime = null;
            this.__state = 'idle';
            this.__replaceState = 'active';
            documentAnimations.delete(this);
            applyAnimationStyles(target);
            if (this.__rejectFinished && !wasFinished) this.__rejectFinished(
                new DOMException('Animation canceled', 'AbortError'));
            this.__finishedPromise = null;
            this.__resolveFinished = this.__rejectFinished = null;
            windowObject.queueMicrotask(() => this.__event('cancel'));
        }
        updatePlaybackRate(rate) { this.playbackRate = rate; }
        persist() {
            this.__replaceState = 'persisted';
            this.__refresh();
        }
        commitStyles() {
            if (!this.effect?.target) throw new DOMException('No target', 'InvalidStateError');
            // A detached or display:none target is not being rendered, so
            // computed effect values cannot be committed to its inline style.
            for (let node = this.effect.target; node instanceof Element; node = node.parentElement)
                if (!node.isConnected || getComputedStyle(node).display === 'none')
                    throw new DOMException('Animation target is not rendered', 'InvalidStateError');
            const values = sampleAnimationValues(this.effect,
                sampleAnimationProgress(this.effect.__timing, this.currentTime), this.__underlying);
            for (const [property, value] of values) this.effect.target.style.setProperty(property, value);
        }
    }
    document.timeline = new DocumentTimeline();
    Element.prototype.animate = function (keyframes, options = {}) {
        const settings = options !== null && typeof options === 'object' ? options : {};
        const timeline = 'timeline' in settings ? settings.timeline : document.timeline;
        const animation = new Animation(new KeyframeEffect(this, keyframes, options), timeline);
        animation.id = 'id' in settings ? settings.id : '';
        animation.play();
        return animation;
    };
    Element.prototype.getAnimations = function (options = {}) {
        const subtree = !!Object(options).subtree;
        const includes = target => {
            if (!subtree) return target === this;
            for (let node = target; node; node = node.parentNode ?? node.host)
                if (node === this) return true;
            return false;
        };
        return [...documentAnimations].filter(animation => includes(animation.effect?.target) &&
            animation.replaceState !== 'removed' &&
            sampleAnimationProgress(animation.effect.__timing, animation.currentTime) !== null);
    };
    document.getAnimations = () => [...documentAnimations].filter(animation =>
        animation.effect?.target && animation.effect.target.isConnected &&
        animation.replaceState !== 'removed' &&
        sampleAnimationProgress(animation.effect.__timing, animation.currentTime) !== null);
    Object.assign(windowObject, { Animation, AnimationEffect, AnimationPlaybackEvent,
        AnimationTimeline, DocumentTimeline, KeyframeEffect });
