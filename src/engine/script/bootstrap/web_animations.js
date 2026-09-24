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
            if (animation.effect?.target !== target) continue;
            const progress = sampleAnimationProgress(animation.effect.__timing, animation.currentTime);
            for (const [property, value] of sampleAnimationValues(animation.effect,
                progress, animation.__underlying)) declarations.set(property, value);
        }
        const style = [...declarations].map(([name, value]) => `${name}: ${value};`).join(' ');
        if (animationAppliedStyles.get(target) === style) return;
        animationAppliedStyles.set(target, style);
        host('setAnimationStyle', nodeId(target), style);
    };
    const scheduleAnimationTick = () => {
        if (animationFrameHandle !== null || ![...documentAnimations].some(animation =>
            animation.playState === 'running' && animation.playbackRate !== 0)) return;
        animationFrameHandle = windowObject.requestAnimationFrame(() => {
            animationFrameHandle = null;
            for (const animation of [...documentAnimations]) animation.__tick();
            scheduleAnimationTick();
        });
    };
    class AnimationPlaybackEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            this.currentTime = init.currentTime ?? null;
            this.timelineTime = init.timelineTime ?? null;
        }
    }
    class DocumentTimeline {
        constructor(options = {}) {
            this.originTime = Number(options?.originTime ?? 0);
        }
        get currentTime() { return Math.max(0, animationNow() - this.originTime); }
    }
    class Animation extends EventTarget {
        constructor(effect = null, timeline = document.timeline) {
            super();
            if (effect !== null && !(effect instanceof KeyframeEffect))
                throw new TypeError('Animation effect must be a KeyframeEffect or null');
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
            this.id = '';
            this.onfinish = null;
            this.oncancel = null;
            this.effect = effect;
        }
        get effect() { return this.__effect; }
        set effect(value) {
            if (value !== null && !(value instanceof KeyframeEffect))
                throw new TypeError('Animation effect must be a KeyframeEffect or null');
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
        set timeline(value) { this.__timeline = value; }
        get playbackRate() { return this.__rate; }
        set playbackRate(value) {
            const current = this.currentTime;
            const rate = Number(value);
            if (!Number.isFinite(rate)) throw new TypeError('Playback rate must be finite');
            this.__rate = rate;
            this.__holdTime = current;
            this.__anchor = animationNow();
            this.__refresh();
        }
        get currentTime() {
            if (this.__holdTime === null) return null;
            return this.__state === 'running' ?
                this.__holdTime + (animationNow() - this.__anchor) * this.__rate : this.__holdTime;
        }
        set currentTime(value) {
            this.__holdTime = value == null ? null : Number(value);
            if (this.__holdTime !== null && !Number.isFinite(this.__holdTime))
                throw new TypeError('Animation currentTime must be finite');
            this.__anchor = animationNow();
            this.__refresh();
        }
        get startTime() {
            return this.__state === 'running' && this.__rate ?
                this.__anchor - this.__holdTime / this.__rate : null;
        }
        set startTime(value) {
            if (value === null) { this.pause(); return; }
            const start = Number(value);
            if (!Number.isFinite(start)) throw new TypeError('Animation startTime must be finite');
            this.__holdTime = 0;
            this.__anchor = start;
            this.__state = 'running';
            documentAnimations.add(this);
            this.__refresh();
        }
        get playState() { return this.__state; }
        get pending() { return false; }
        get replaceState() { return 'active'; }
        get ready() { return Promise.resolve(this); }
        get finished() {
            if (this.__state === 'finished') return Promise.resolve(this);
            if (!this.__finishedPromise) this.__finishedPromise = new Promise((resolve, reject) => {
                this.__resolveFinished = resolve;
                this.__rejectFinished = reject;
            });
            return this.__finishedPromise;
        }
        __event(type) {
            const event = new AnimationPlaybackEvent(type, {
                currentTime: this.currentTime, timelineTime: this.timeline?.currentTime ?? null
            });
            this.dispatchEvent(event);
            const handler = type === 'finish' ? this.onfinish : this.oncancel;
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
        __tick() {
            if (this.__state !== 'running') return;
            const end = animationEndTime(this.effect?.__timing ?? normalizeAnimationTiming(0));
            const current = this.currentTime;
            if (this.__rate >= 0 && current >= end || this.__rate < 0 && current <= 0) {
                this.finish();
            } else applyAnimationStyles(this.effect?.target);
        }
        play() {
            const end = animationEndTime(this.effect?.__timing ?? normalizeAnimationTiming(0));
            const current = this.currentTime;
            this.__holdTime = current === null || this.__rate >= 0 && current >= end ?
                (this.__rate < 0 ? end : 0) : current;
            this.__anchor = animationNow();
            this.__state = 'running';
            this.__finishedPromise = null;
            this.__refresh();
        }
        pause() {
            if (this.__state === 'paused') return;
            this.__holdTime = this.currentTime ?? 0;
            this.__state = 'paused';
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
            this.__holdTime = this.__rate < 0 ? 0 : end;
            this.__state = 'finished';
            this.__refresh();
            this.__resolveFinished?.(this);
            this.__finishedPromise = null;
            if (!['forwards', 'both'].includes(this.effect?.__timing.fill))
                documentAnimations.delete(this);
            windowObject.queueMicrotask(() => this.__event('finish'));
        }
        cancel() {
            if (this.__state === 'idle') return;
            const target = this.effect?.target;
            this.__holdTime = null;
            this.__state = 'idle';
            documentAnimations.delete(this);
            applyAnimationStyles(target);
            if (this.__rejectFinished) this.__rejectFinished(
                new DOMException('Animation canceled', 'AbortError'));
            this.__finishedPromise = null;
            this.__resolveFinished = this.__rejectFinished = null;
            windowObject.queueMicrotask(() => this.__event('cancel'));
        }
        updatePlaybackRate(rate) { this.playbackRate = rate; }
        persist() {}
        commitStyles() {
            if (!this.effect?.target) throw new DOMException('No target', 'InvalidStateError');
            const values = sampleAnimationValues(this.effect,
                sampleAnimationProgress(this.effect.__timing, this.currentTime), this.__underlying);
            for (const [property, value] of values) this.effect.target.style.setProperty(property, value);
        }
    }
    document.timeline = new DocumentTimeline();
    Element.prototype.animate = function (keyframes, options = {}) {
        const animation = new Animation(new KeyframeEffect(this, keyframes, options), document.timeline);
        animation.play();
        return animation;
    };
    Element.prototype.getAnimations = function () {
        return [...documentAnimations].filter(animation => animation.effect?.target === this &&
            sampleAnimationProgress(animation.effect.__timing, animation.currentTime) !== null);
    };
    document.getAnimations = () => [...documentAnimations].filter(animation =>
        animation.effect?.target && animation.effect.target.isConnected &&
        sampleAnimationProgress(animation.effect.__timing, animation.currentTime) !== null);
    Object.assign(windowObject, { Animation, AnimationPlaybackEvent, DocumentTimeline, KeyframeEffect });
