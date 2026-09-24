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
    const animationIdlProperty = property => property.startsWith('--') ? property :
        property === 'float' ? 'cssFloat' :
        property.replace(/-([a-z])/g, (_, letter) => letter.toUpperCase());
    const animationValidValue = (property, value) =>
        host('cssSupports', `(${property}: ${value})`);
    const animationFrameValues = input => {
        const values = new Map();
        for (const [name, value] of Object.entries(Object(input))) {
            if (animationMetadata.has(name)) continue;
            const property = animationProperty(name);
            if (property !== null && value !== null && value !== undefined &&
                animationValidValue(property, String(value)))
                values.set(property, String(value));
        }
        return values;
    };
    const normalizeAnimationComposite = value => {
        if (value === undefined || value === 'auto' || value === 'replace') return 'replace';
        throw new DOMException('Only replace compositing is supported', 'NotSupportedError');
    };
    const computeAnimationOffsets = frames => {
        if (!frames.length) return;
        for (const frame of frames) frame.computedOffset = frame.offset;
        if (frames.length > 1 && frames[0].computedOffset === null)
            frames[0].computedOffset = 0;
        if (frames.at(-1).computedOffset === null) frames.at(-1).computedOffset = 1;
        for (let index = 0; index < frames.length;) {
            if (frames[index].computedOffset !== null) { index++; continue; }
            const start = index - 1;
            while (index < frames.length && frames[index].computedOffset === null) index++;
            const begin = frames[start].computedOffset, end = frames[index].computedOffset;
            for (let inner = start + 1; inner < index; inner++)
                frames[inner].computedOffset = begin + (end - begin) *
                    (inner - start) / (index - start);
        }
    };
    const normalizePropertyIndexedFrames = source => {
        const byOffset = new Map();
        for (const [name, raw] of Object.entries(source)) {
            if (animationMetadata.has(name)) continue;
            const property = animationProperty(name);
            if (property === null) continue;
            const values = Array.isArray(raw) ? raw : [raw];
            for (let index = 0; index < values.length; index++) {
                if (values[index] === undefined || values[index] === null ||
                    !animationValidValue(property, String(values[index]))) continue;
                const offset = values.length === 1 ? 1 : index / (values.length - 1);
                if (!byOffset.has(offset)) byOffset.set(offset, {
                    offset: null, computedOffset: offset, easing: 'linear', values: new Map()
                });
                byOffset.get(offset).values.set(property, String(values[index]));
            }
        }
        const frames = [...byOffset.values()].sort((left, right) =>
            left.computedOffset - right.computedOffset);
        const offsets = source.offset === undefined ? [] :
            Array.isArray(source.offset) ? source.offset : [source.offset];
        const easings = source.easing === undefined ? ['linear'] :
            Array.isArray(source.easing) ? source.easing : [source.easing];
        if (!easings.length) easings.push('linear');
        const composites = source.composite === undefined ? ['auto'] :
            Array.isArray(source.composite) ? source.composite : [source.composite];
        for (let index = 0; index < frames.length; index++) {
            if (index < offsets.length)
                frames[index].offset = offsets[index] == null ? null : Number(offsets[index]);
            frames[index].easing = normalizeAnimationEasing(easings[index % easings.length]);
            if (composites.length) normalizeAnimationComposite(composites[index % composites.length]);
        }
        if (offsets.length) computeAnimationOffsets(frames);
        return frames;
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
                normalizeAnimationComposite(item.composite);
                return { offset, computedOffset: null,
                    easing: normalizeAnimationEasing(item.easing ?? 'linear'),
                    values: animationFrameValues(item) };
            });
        } else if (typeof source === 'object') {
            frames = normalizePropertyIndexedFrames(source);
        } else throw new TypeError('Keyframes must be an object or array');
        if (frames.length > 64) throw new DOMException('Too many keyframes', 'NotSupportedError');
        let last = -1;
        for (const frame of frames) {
            if (frame.values.size > 32) throw new DOMException('Too many animated properties', 'NotSupportedError');
            if (frame.offset !== null) {
                if (!Number.isFinite(frame.offset) || frame.offset < 0 || frame.offset > 1)
                    throw new TypeError('Keyframe offset must be between 0 and 1');
                if (frame.offset < last) throw new TypeError('Keyframe offsets must be nondecreasing');
                last = frame.offset;
            }
        }
        if (Array.isArray(source)) computeAnimationOffsets(frames);
        return frames;
    };
    const normalizeAnimationEasing = source => {
        const value = String(source).trim().toLowerCase();
        if (/^(linear|ease|ease-in|ease-out|ease-in-out|step-start|step-end)$/.test(value))
            return value;
        if (value.startsWith('linear(') && parseLinearAnimationEasing(value)) return value;
        const bezier = /^cubic-bezier\(\s*([^,]+),\s*([^,]+),\s*([^,]+),\s*([^,]+)\s*\)$/.exec(value);
        if (bezier) {
            const coordinates = bezier.slice(1).map(Number);
            if (coordinates.every(Number.isFinite) && coordinates[0] >= 0 &&
                coordinates[0] <= 1 && coordinates[2] >= 0 && coordinates[2] <= 1)
                return value;
        }
        const steps = /^steps\(\s*(\d+)\s*(?:,\s*(start|end|jump-start|jump-end|jump-none|jump-both))?\s*\)$/.exec(value);
        if (steps && Number(steps[1]) > 0 &&
            (steps[2] !== 'jump-none' || Number(steps[1]) > 1)) return value;
        throw new TypeError('Unsupported animation easing');
    };
    // CSS Easing 2 §2.1: missing input positions are distributed after the
    // explicitly positioned stops have been clamped to nondecreasing order.
    // https://drafts.csswg.org/css-easing-2/#linear-easing-function
    const parseLinearAnimationEasing = source => {
        const match = /^linear\((.*)\)$/.exec(source);
        if (!match) return null;
        const entries = match[1].split(',');
        if (!entries.length || entries.length > 64) return null;
        const number = /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?$/i;
        const percent = /^([+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?)%$/i;
        const points = [];
        for (const entry of entries) {
            const tokens = entry.trim().split(/\s+/);
            if (!number.test(tokens[0]) || tokens.length > 3) return null;
            const output = Number(tokens[0]);
            if (!Number.isFinite(output)) return null;
            if (tokens.length === 1) points.push({x: null, y: output});
            else {
                for (const token of tokens.slice(1)) {
                    const position = percent.exec(token);
                    if (!position || !Number.isFinite(Number(position[1]))) return null;
                    points.push({x: Number(position[1]) / 100, y: output});
                }
            }
        }
        if (points[0].x === null) points[0].x = 0;
        if (points.at(-1).x === null) points.at(-1).x = 1;
        let previous = points[0].x;
        for (const point of points) {
            if (point.x === null) continue;
            point.x = Math.max(point.x, previous);
            previous = point.x;
        }
        for (let index = 0; index < points.length;) {
            if (points[index].x !== null) { index++; continue; }
            const start = index - 1;
            while (points[index]?.x === null) index++;
            const from = points[start].x, to = points[index].x;
            for (let inner = start + 1; inner < index; inner++)
                points[inner].x = from + (to - from) * (inner - start) / (index - start);
        }
        return points;
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
    // The abstract effect interface owns timing. KeyframeEffect adds targets
    // and keyframes but exposes the same timing methods through inheritance.
    class AnimationEffect {
        constructor() {
            if (new.target === AnimationEffect) throw new TypeError('Illegal constructor');
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
    class KeyframeEffect extends AnimationEffect {
        constructor(target, keyframes, options = {}) {
            super();
            if (target instanceof KeyframeEffect && arguments.length === 1) {
                // The copy constructor owns an independent keyframe/timing set.
                // Mutating either effect must never mutate the other animation.
                this.__target = target.__target;
                this.__frames = target.__frames.map(frame => ({...frame,
                    values: new Map(frame.values)}));
                this.__timing = {...target.__timing};
                this.__animation = null;
                this.__composite = target.__composite;
                return;
            }
            if (!(target instanceof Element) && target !== null)
                throw new TypeError('KeyframeEffect target must be an Element or null');
            if (options && typeof options === 'object') {
                if (options.pseudoElement != null)
                    throw new DOMException('Pseudo-element animation is not supported', 'NotSupportedError');
                if (options.composite !== undefined) normalizeAnimationComposite(options.composite);
            }
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
            if (this.__target === value) return;
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
        get pseudoElement() { return null; }
        set pseudoElement(value) {
            if (value != null)
                throw new DOMException('Pseudo-element animation is not supported', 'NotSupportedError');
        }
        getKeyframes() {
            return this.__frames.map(frame => ({ offset: frame.offset,
                computedOffset: frame.computedOffset, easing: frame.easing, composite: 'replace',
                ...Object.fromEntries([...frame.values].map(([property, value]) =>
                    [animationIdlProperty(property), value])) }));
        }
        setKeyframes(keyframes) {
            this.__frames = normalizeAnimationFrames(keyframes);
            this.__animation?.__rekeyframe();
        }
    }
