    const cubicAnimationValue = (a, b, c, t) =>
        ((1 - t) ** 3 * 0) + 3 * (1 - t) ** 2 * t * a +
        3 * (1 - t) * t * t * b + t ** 3;
    const cubicAnimationEasing = (x1, y1, x2, y2, progress) => {
        if (progress <= 0 || progress >= 1) return progress;
        let low = 0, high = 1;
        for (let step = 0; step < 18; step++) {
            const middle = (low + high) / 2;
            if (cubicAnimationValue(x1, x2, 0, middle) < progress) low = middle;
            else high = middle;
        }
        return cubicAnimationValue(y1, y2, 0, (low + high) / 2);
    };
    const animationEasingProgress = (name, progress) => {
        const builtins = {
            ease: [0.25, 0.1, 0.25, 1], 'ease-in': [0.42, 0, 1, 1],
            'ease-out': [0, 0, 0.58, 1], 'ease-in-out': [0.42, 0, 0.58, 1]
        };
        if (name === 'linear') return progress;
        if (builtins[name]) return cubicAnimationEasing(...builtins[name], progress);
        if (name === 'step-start') return Math.ceil(progress);
        if (name === 'step-end') return Math.floor(progress);
        if (name.startsWith('cubic-bezier(')) {
            const values = name.slice(13, -1).split(',').map(Number);
            return cubicAnimationEasing(...values, progress);
        }
        const match = /^steps\(\s*(\d+)\s*(?:,\s*(\S+)\s*)?\)$/.exec(name);
        if (match) {
            const count = Number(match[1]);
            const start = ['start', 'jump-start'].includes(match[2]);
            return Math.min(1, (start ? Math.ceil(progress * count) : Math.floor(progress * count)) / count);
        }
        return progress;
    };
    // Timing is sampled statelessly. Interval endpoints are exclusive while
    // filling holds the final keyframe of an integral last iteration.
    // https://www.w3.org/TR/web-animations-1/#core-animation-effect-calculations
    const sampleAnimationTiming = (timing, currentTime) => {
        const unresolved = {progress: null, currentIteration: null};
        if (currentTime === null) return unresolved;
        const duration = animationDuration(timing);
        const activeDuration = animationActiveDuration(timing);
        const endTime = Math.max(0, timing.delay + activeDuration + timing.endDelay);
        const afterBoundary = Math.min(timing.delay + activeDuration, endTime);
        const phase = currentTime < Math.min(timing.delay, endTime) ? 'before' :
            currentTime >= afterBoundary ? 'after' : 'active';
        let activeTime;
        if (phase === 'before') {
            if (!['backwards', 'both'].includes(timing.fill)) return unresolved;
            activeTime = Math.max(currentTime - timing.delay, 0);
        } else if (phase === 'after') {
            if (!['forwards', 'both'].includes(timing.fill)) return unresolved;
            activeTime = Math.max(Math.min(currentTime - timing.delay, activeDuration), 0);
        } else activeTime = currentTime - timing.delay;
        const overall = (duration === 0 ? phase === 'before' ? 0 : timing.iterations :
            activeTime / duration) + timing.iterationStart;
        let simple = (overall === Infinity ? timing.iterationStart : overall) % 1;
        if (simple === 0 && phase !== 'before' && activeTime === activeDuration &&
            timing.iterations !== 0) simple = 1;
        const currentIteration = phase === 'after' && timing.iterations === Infinity ?
            Infinity : Math.max(0, Math.floor(overall) - (simple === 1 ? 1 : 0));
        const reversed = timing.direction === 'reverse' ||
            (timing.direction === 'alternate' && currentIteration % 2 === 1) ||
            (timing.direction === 'alternate-reverse' && currentIteration % 2 === 0);
        const directed = reversed ? 1 - simple : simple;
        return {progress: animationEasingProgress(timing.easing, directed), currentIteration};
    };
    const sampleAnimationProgress = (timing, currentTime) =>
        sampleAnimationTiming(timing, currentTime).progress;
    const animationNumber = /^([+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?)([a-z%]*)$/i;
    const animationColor = source => {
        const normalized = String(host('normalizeCssColor', source));
        const parts = normalized.split('\x1f');
        return parts.length === 5 ? parts.slice(1).map(Number) : null;
    };
    const interpolateAnimationValue = (property, from, to, progress) => {
        if (progress <= 0) return from;
        if (progress >= 1) return to;
        if (property === 'transform') {
            const transform = interpolateAnimationTransform(from, to, progress);
            if (transform !== null) return transform;
        }
        const first = animationNumber.exec(String(from).trim());
        const last = animationNumber.exec(String(to).trim());
        if (first && last && first[2] === last[2]) {
            const value = Number(first[1]) + (Number(last[1]) - Number(first[1])) * progress;
            return String(Math.round(value * 10000) / 10000) + first[2];
        }
        if (property.includes('color') || property === 'fill' || property === 'stroke') {
            const a = animationColor(from), b = animationColor(to);
            if (a && b) {
                const alphaA = a[3] / 255, alphaB = b[3] / 255;
                const alpha = alphaA + (alphaB - alphaA) * progress;
                const channel = index => alpha === 0 ? 0 : Math.round(
                    (a[index] * alphaA * (1 - progress) + b[index] * alphaB * progress) / alpha);
                return `rgba(${channel(0)}, ${channel(1)}, ${channel(2)}, ${alpha})`;
            }
        }
        // CSS Transitions §4: properties without a supported interpolation use
        // a discrete switch at the 50% midpoint, not an invented numeric blend.
        return progress < 0.5 ? from : to;
    };
    const animationUnderlyings = effect => {
        const target = effect.target, values = new Map();
        if (!target) return values;
        const style = getComputedStyle(target);
        for (const frame of effect.__frames) for (const property of frame.values.keys()) {
            if (!values.has(property)) values.set(property, style.getPropertyValue(property));
        }
        return values;
    };
    const sampleAnimationValues = (effect, progress, underlying) => {
        const values = new Map();
        if (progress === null || !effect?.target) return values;
        const properties = new Set(effect.__frames.flatMap(frame => [...frame.values.keys()]));
        for (const property of properties) {
            const frames = effect.__frames.filter(frame => frame.values.has(property))
                .map(frame => ({ offset: frame.offset, value: frame.values.get(property),
                    easing: frame.easing }));
            if (!frames.length) continue;
            if (frames[0].offset > 0) frames.unshift({ offset: 0,
                value: underlying.get(property) ?? '', easing: 'linear' });
            if (frames.at(-1).offset < 1) frames.push({ offset: 1,
                value: underlying.get(property) ?? '', easing: 'linear' });
            let start = frames[0], end = frames.at(-1);
            for (let index = 1; index < frames.length; index++) {
                if (progress <= frames[index].offset) {
                    start = frames[index - 1]; end = frames[index]; break;
                }
            }
            const segment = end.offset === start.offset ? 1 :
                (progress - start.offset) / (end.offset - start.offset);
            const eased = animationEasingProgress(start.easing, Math.max(0, Math.min(1, segment)));
            values.set(property, interpolateAnimationValue(property, start.value, end.value, eased));
        }
        return values;
    };
