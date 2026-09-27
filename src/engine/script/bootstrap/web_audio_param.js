    const audioParamToken = {};
    const audioParamState = new WeakMap();
    let nextAutomationEvent = 1;
    const scheduleAudioParamEvent = (param, event) => {
        const state = audioParamState.get(param);
        if (state.events.length >= 4096)
            throw new DOMException('Too many scheduled automation events', 'NotSupportedError');
        const contextState = audioContextState.get(state.context);
        const curveBytes = event.type === 'curve' ? event.values.byteLength : 0;
        if (curveBytes > MAX_AUDIO_AUTOMATION_CURVE_BYTES -
            contextState.automationCurveBytes)
            throw new DOMException('Audio automation curves exceed the context memory limit',
                'NotSupportedError');
        const end = event.type === 'curve' ? event.time + event.duration : event.time;
        for (const existing of state.events) {
            if (existing.type === 'curve' &&
                event.time < (existing.holdTime ?? existing.time + existing.duration) &&
                end > existing.time)
                throw new DOMException('Automation events overlap a value curve', 'NotSupportedError');
            if (event.type === 'curve' && existing.time > event.time && existing.time < end)
                throw new DOMException('Value curve overlaps an automation event', 'NotSupportedError');
        }
        event.order = nextAutomationEvent++;
        state.events.push(event);
        state.events.sort((a, b) => a.time - b.time || a.order - b.order);
        contextState.automationCurveBytes += curveBytes;
        state.timelineDirty = true;
        return param;
    };
    const audioParamActiveValue = (active, value, time) => {
        if (!active) return value;
        if (active.type === 'target')
            return active.value + (active.initial - active.value) *
                Math.exp(-(time - active.time) / active.constant);
        const position = Math.min(1, Math.max(0,
            (Math.min(time, active.holdTime ?? Infinity) - active.time) / active.duration));
        const index = position * (active.values.length - 1);
        const lower = Math.floor(index), upper = Math.min(lower + 1, active.values.length - 1);
        return active.values[lower] +
            (active.values[upper] - active.values[lower]) * (index - lower);
    };
    const audioParamTimeline = state => {
        if (!state.timelineDirty) return;
        let value = state.intrinsic;
        let active = null;
        state.checkpoints = state.events.map(event => {
            value = audioParamActiveValue(active, value, event.time);
            active = null;
            if (event.type === 'set' || event.type === 'linear' ||
                event.type === 'exponential') value = event.value;
            else if (event.type === 'target') active = { ...event, initial: value };
            else if (event.type === 'curve') active = event;
            return { value, active };
        });
        state.timelineDirty = false;
    };
    const audioParamValueAt = (param, time) => {
        const state = audioParamState.get(param);
        audioParamTimeline(state);
        // The sorted event timeline is resolved once after an edit, then each
        // DSP sample finds its neighboring events in logarithmic time.
        let low = 0, high = state.events.length;
        while (low < high) {
            const middle = (low + high) >>> 1;
            if (state.events[middle].time <= time) low = middle + 1;
            else high = middle;
        }
        const previous = low ? state.checkpoints[low - 1] :
            { value: state.intrinsic, active: null };
        const anchor = low ? state.events[low - 1].time : 0;
        const next = state.events[low];
        if (next?.type === 'linear' && next.time > anchor)
            return previous.value + (next.value - previous.value) *
                (time - anchor) / (next.time - anchor);
        if (next?.type === 'exponential' && next.time > anchor && previous.value > 0)
            return previous.value * Math.pow(next.value / previous.value,
                (time - anchor) / (next.time - anchor));
        return audioParamActiveValue(previous.active, previous.value, time);
    };
    const audioParamValueForFrame = (param, frame, offset, sampleRate, cache) => {
        const state = audioParamState.get(param);
        let value = audioParamValueAt(param, (frame +
            (state.rate === 'k-rate' ? 0 : offset)) / sampleRate);
        if (cache && state.inputs.size) {
            const frames = Math.min(AUDIO_QUANTUM,
                audioContextState.get(state.context).length - frame);
            const index = state.rate === 'k-rate' ? 0 : offset;
            // Web Audio 1.0 §1.5.5/§1.6.3: connected outputs downmix to
            // mono and sum with the intrinsic automation trajectory.
            for (const edge of state.inputs) {
                const samples = renderAudioNode(state.context, edge.source,
                    frame, frames, cache, edge.output);
                let mono = 0;
                for (const channel of samples) mono += channel[index] / samples.length;
                value += mono;
            }
        }
        if (Number.isNaN(value)) value = param.defaultValue;
        // Computed DSP values are clamped to the nominal range; the exposed
        // automation trajectory and AudioParam.value remain unmodified.
        return Math.max(state.minValue, Math.min(state.maxValue, value));
    };

    class AudioParam {
        constructor(token, context, defaultValue, minValue, maxValue,
            rate = 'a-rate', fixedRate = false) {
            if (token !== audioParamToken) throw new TypeError('Illegal constructor');
            audioParamState.set(this,
                { context, intrinsic: defaultValue, minValue, maxValue,
                    events: [], checkpoints: [], timelineDirty: false,
                    rate, fixedRate, inputs: new Set(), owner: null });
            Object.defineProperties(this, {
                defaultValue: { enumerable: true, value: defaultValue },
                minValue: { enumerable: true, value: minValue },
                maxValue: { enumerable: true, value: maxValue }
            });
        }
        get value() {
            return audioParamValueAt(this, audioParamState.get(this).context.currentTime);
        }
        set value(value) {
            value = finiteFloat(value, 'AudioParam value');
            const state = audioParamState.get(this);
            state.intrinsic = value;
            scheduleAudioParamEvent(this,
                { type: 'set', value, time: state.context.currentTime });
        }
        get automationRate() { return audioParamState.get(this).rate; }
        set automationRate(value) {
            if (value !== 'a-rate' && value !== 'k-rate')
                throw new TypeError('Invalid automation rate');
            const state = audioParamState.get(this);
            if (state.fixedRate && value !== state.rate)
                throw new DOMException('This AudioParam has a fixed automation rate',
                    'InvalidStateError');
            state.rate = value;
        }
        setValueAtTime(value, startTime) {
            return scheduleAudioParamEvent(this, { type: 'set', value: finiteFloat(value, 'value'),
                time: nonnegative(startTime, 'startTime') });
        }
        linearRampToValueAtTime(value, endTime) {
            return scheduleAudioParamEvent(this, { type: 'linear', value: finiteFloat(value, 'value'),
                time: nonnegative(endTime, 'endTime') });
        }
        exponentialRampToValueAtTime(value, endTime) {
            value = positiveFloat(value, 'value');
            return scheduleAudioParamEvent(this, { type: 'exponential', value,
                time: nonnegative(endTime, 'endTime') });
        }
        setTargetAtTime(target, startTime, timeConstant) {
            return scheduleAudioParamEvent(this, { type: 'target',
                value: finiteFloat(target, 'target'),
                time: nonnegative(startTime, 'startTime'),
                constant: positive(timeConstant, 'timeConstant') });
        }
        setValueCurveAtTime(values, startTime, duration) {
            if (values == null || typeof values[Symbol.iterator] !== 'function')
                throw new TypeError('Value curve must be a sequence of float samples');
            const samples = [];
            for (const value of values) {
                if (samples.length >= 65_536)
                    throw new DOMException('Value curve is too large', 'NotSupportedError');
                samples.push(finiteFloat(value, 'curve sample'));
            }
            if (samples.length < 2)
                throw new DOMException('Value curve requires at least two samples',
                    'InvalidStateError');
            const time = nonnegative(startTime, 'startTime');
            duration = positive(duration, 'duration');
            if (!Number.isFinite(time + duration)) throw new TypeError('Curve end must be finite');
            return scheduleAudioParamEvent(this, { type: 'curve', values: new Float32Array(samples),
                time, duration });
        }
        cancelScheduledValues(cancelTime) {
            cancelTime = nonnegative(cancelTime, 'cancelTime');
            const state = audioParamState.get(this);
            const contextState = audioContextState.get(state.context);
            state.events = state.events.filter(event => {
                const keep = event.time < cancelTime &&
                    (event.type !== 'curve' || event.time + event.duration < cancelTime);
                if (!keep && event.type === 'curve')
                    contextState.automationCurveBytes -= event.values.byteLength;
                return keep;
            });
            state.timelineDirty = true;
            return this;
        }
        cancelAndHoldAtTime(cancelTime) {
            cancelTime = nonnegative(cancelTime, 'cancelTime');
            const state = audioParamState.get(this);
            const held = audioParamValueAt(this, cancelTime);
            const next = state.events.find(event => event.time > cancelTime);
            const curve = state.events.find(event => event.type === 'curve' &&
                event.time <= cancelTime && event.time + event.duration >= cancelTime);
            const contextState = audioContextState.get(state.context);
            state.events = state.events.filter(event => {
                const keep = event.time <= cancelTime;
                if (!keep && event.type === 'curve')
                    contextState.automationCurveBytes -= event.values.byteLength;
                return keep;
            });
            if (curve) curve.holdTime = cancelTime;
            state.timelineDirty = true;
            // Retain the shape of a ramp leading to the hold time; replacing it
            // with only setValueAtTime would incorrectly flatten its earlier samples.
            if (next?.type === 'linear' || next?.type === 'exponential')
                return scheduleAudioParamEvent(this,
                    { type: next.type, value: held, time: cancelTime });
            return scheduleAudioParamEvent(this,
                { type: 'set', value: held, time: cancelTime });
        }
    }
