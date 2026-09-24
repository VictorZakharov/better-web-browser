    // Web Animations Level 1 keyframe normalization. Keep author keyframes apart
    // from DOM style attributes; the animation origin is applied by the CSS host.
    // https://www.w3.org/TR/web-animations-1/#the-keyframeeffect-interface
    const animationMetadata = new Set(['offset', 'easing', 'composite', 'computedOffset']);
    const animationProperty = name => {
        name = String(name);
        const property = name.startsWith('--') ? name : name.includes('-') ? name.toLowerCase() :
            name === 'cssFloat' ? 'float' : name.replace(/[A-Z]/g, match => '-' + match.toLowerCase());
        return host('cssPropertySupported', property) ? property : null;
    };
    const animationFrameValues = input => {
        const values = new Map();
        for (const [name, value] of Object.entries(Object(input))) {
            if (animationMetadata.has(name)) continue;
            const property = animationProperty(name);
            if (property !== null && value !== null && value !== undefined)
                values.set(property, String(value));
        }
        return values;
    };
    const normalizeAnimationFrames = source => {
        if (source == null) return [];
        let frames;
        if (Array.isArray(source)) {
            frames = source.map(item => {
                if (item == null || typeof item !== 'object')
                    throw new TypeError('Keyframes must be objects');
                const offset = item.offset == null ? null : Number(item.offset);
                if (offset !== null && (!Number.isFinite(offset) || offset < 0 || offset > 1))
                    throw new TypeError('Keyframe offset must be between 0 and 1');
                return { offset, easing: normalizeAnimationEasing(item.easing ?? 'linear'),
                    values: animationFrameValues(item) };
            });
        } else if (typeof source === 'object') {
            const entries = Object.entries(source).filter(([name]) => !animationMetadata.has(name));
            const count = Math.max(0, ...entries.map(([, value]) => Array.isArray(value) ? value.length : 1));
            frames = Array.from({ length: count }, (_, index) => ({
                offset: count <= 1 ? 1 : index / (count - 1), easing: 'linear', values: new Map()
            }));
            for (const [name, raw] of entries) {
                const property = animationProperty(name);
                if (property === null) continue;
                const values = Array.isArray(raw) ? raw : [raw];
                for (let index = 0; index < values.length; index++) {
                    const target = count <= 1 ? 0 : Math.round(index * (count - 1) / Math.max(1, values.length - 1));
                    if (values[index] != null) frames[target].values.set(property, String(values[index]));
                }
            }
            if (source.easing !== undefined) {
                const easing = Array.isArray(source.easing) ? source.easing : [source.easing];
                frames.forEach((frame, index) => {
                    frame.easing = normalizeAnimationEasing(easing[index % easing.length]);
                });
            }
        } else throw new TypeError('Keyframes must be an object or array');
        if (frames.length > 64) throw new DOMException('Too many keyframes', 'NotSupportedError');
        let last = -1;
        for (const frame of frames) {
            if (frame.values.size > 32) throw new DOMException('Too many animated properties', 'NotSupportedError');
            if (frame.offset !== null) {
                if (frame.offset < last) throw new TypeError('Keyframe offsets must be nondecreasing');
                last = frame.offset;
            }
        }
        if (frames.length === 0) return frames;
        if (frames[0].offset === null) frames[0].offset = 0;
        if (frames.at(-1).offset === null) frames.at(-1).offset = 1;
        for (let index = 0; index < frames.length;) {
            if (frames[index].offset !== null) { index++; continue; }
            const start = index - 1;
            while (index < frames.length && frames[index].offset === null) index++;
            const begin = frames[start].offset, end = frames[index].offset;
            for (let inner = start + 1; inner < index; inner++)
                frames[inner].offset = begin + (end - begin) * (inner - start) / (index - start);
        }
        return frames;
    };
    const normalizeAnimationEasing = source => {
        const value = String(source).trim().toLowerCase();
        if (/^(linear|ease|ease-in|ease-out|ease-in-out|step-start|step-end)$/.test(value))
            return value;
        const bezier = /^cubic-bezier\(\s*([^,]+),\s*([^,]+),\s*([^,]+),\s*([^,]+)\s*\)$/.exec(value);
        if (bezier) {
            const coordinates = bezier.slice(1).map(Number);
            if (coordinates.every(Number.isFinite) && coordinates[0] >= 0 &&
                coordinates[0] <= 1 && coordinates[2] >= 0 && coordinates[2] <= 1)
                return value;
        }
        const steps = /^steps\(\s*(\d+)\s*(?:,\s*(start|end|jump-start|jump-end))?\s*\)$/.exec(value);
        if (steps && Number(steps[1]) > 0) return value;
        throw new TypeError('Unsupported animation easing');
    };
    const normalizeAnimationTiming = options => {
        if (typeof options === 'number') options = { duration: options };
        else options = options == null ? {} : Object(options);
        const duration = options.duration === undefined || options.duration === 'auto'
            ? 'auto' : Number(options.duration);
        const delay = Number(options.delay ?? 0);
        const endDelay = Number(options.endDelay ?? 0);
        const iterations = Number(options.iterations ?? 1);
        const iterationStart = Number(options.iterationStart ?? 0);
        if (duration !== 'auto' && (Number.isNaN(duration) || duration < 0) || !Number.isFinite(delay) ||
            !Number.isFinite(endDelay) || iterations < 0 || Number.isNaN(iterations) ||
            !Number.isFinite(iterationStart) || iterationStart < 0)
            throw new TypeError('Invalid animation timing');
        const fill = String(options.fill ?? 'auto');
        const direction = String(options.direction ?? 'normal');
        if (!['auto', 'none', 'forwards', 'backwards', 'both'].includes(fill) ||
            !['normal', 'reverse', 'alternate', 'alternate-reverse'].includes(direction))
            throw new TypeError('Invalid animation fill or direction');
        return { delay, endDelay, duration, iterations, iterationStart, fill, direction,
            easing: normalizeAnimationEasing(options.easing ?? 'linear') };
    };
    const animationDuration = timing => timing.duration === 'auto' ? 0 : timing.duration;
    const animationActiveDuration = timing => {
        const duration = animationDuration(timing);
        return duration === 0 || timing.iterations === 0 ? 0 : duration * timing.iterations;
    };
    class KeyframeEffect {
        constructor(target, keyframes, options = {}) {
            if (!(target instanceof Element) && target !== null)
                throw new TypeError('KeyframeEffect target must be an Element or null');
            this.__target = target;
            this.__frames = normalizeAnimationFrames(keyframes);
            this.__timing = normalizeAnimationTiming(options);
            this.__animation = null;
            this.__composite = 'replace';
        }
        get target() { return this.__target; }
        set target(value) {
            if (!(value instanceof Element) && value !== null)
                throw new TypeError('KeyframeEffect target must be an Element or null');
            const previous = this.__target;
            this.__target = value;
            this.__animation?.__retarget(previous);
        }
        get composite() { return this.__composite; }
        set composite(value) {
            if (String(value) !== 'replace')
                throw new DOMException('Only replace compositing is supported', 'NotSupportedError');
            this.__composite = 'replace';
        }
        getKeyframes() {
            return this.__frames.map(frame => ({ offset: frame.offset,
                computedOffset: frame.offset, easing: frame.easing, composite: 'replace',
                ...Object.fromEntries(frame.values) }));
        }
        setKeyframes(keyframes) {
            this.__frames = normalizeAnimationFrames(keyframes);
            this.__animation?.__refresh();
        }
        getTiming() { return { ...this.__timing }; }
        updateTiming(options = {}) {
            this.__timing = normalizeAnimationTiming({ ...this.__timing, ...Object(options) });
            this.__animation?.__refresh();
        }
        getComputedTiming() {
            const duration = animationDuration(this.__timing);
            const activeDuration = animationActiveDuration(this.__timing);
            const localTime = this.__animation?.currentTime ?? null;
            const sample = sampleAnimationTiming(this.__timing, localTime);
            return { ...this.__timing, duration,
                fill: this.__timing.fill === 'auto' ? 'none' : this.__timing.fill,
                activeDuration,
                endTime: Math.max(0, this.__timing.delay + activeDuration + this.__timing.endDelay),
                localTime, progress: sample.progress, currentIteration: sample.currentIteration };
        }
    }
