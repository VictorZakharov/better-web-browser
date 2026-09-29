    const spatialPannerState = new WeakMap();
    const spatialModel = value => {
        if (value === 'equalpower') return value;
        if (value === 'HRTF') throw new AudioDOMException(
            'HRTF impulse responses are not available', 'NotSupportedError');
        throw new TypeError('Invalid panning model');
    };
    const spatialDistanceModel = value => {
        if (['linear', 'inverse', 'exponential'].includes(value)) return value;
        throw new TypeError('Invalid distance model');
    };
    const spatialNumber = (value, name, minimum = -Infinity) => {
        const number = finite(value, name);
        if (number < minimum) throw new RangeError(name + ' is out of range');
        return number;
    };
    const spatialMaximumDistance = value => {
        const number = spatialNumber(value, 'maxDistance');
        if (number <= 0) throw new RangeError('maxDistance must be positive');
        return number;
    };
    const spatialOuterGain = value => {
        const number = finite(value, 'coneOuterGain');
        if (number < 0 || number > 1) throw new AudioDOMException(
            'coneOuterGain must be in [0, 1]', 'InvalidStateError');
        return number;
    };
    const spatialOption = (options, name, fallback) => {
        const value = options[name];
        return value === undefined ? fallback : value;
    };
    const spatialOptions = options => ({
        panningModel: spatialModel(spatialOption(options, 'panningModel', 'equalpower')),
        distanceModel: spatialDistanceModel(spatialOption(options, 'distanceModel', 'inverse')),
        refDistance: spatialNumber(spatialOption(options, 'refDistance', 1),
            'refDistance', 0),
        maxDistance: spatialMaximumDistance(spatialOption(options, 'maxDistance', 10000)),
        rolloffFactor: spatialNumber(spatialOption(options, 'rolloffFactor', 1),
            'rolloffFactor', 0),
        coneInnerAngle: spatialNumber(spatialOption(options, 'coneInnerAngle', 360),
            'coneInnerAngle'),
        coneOuterAngle: spatialNumber(spatialOption(options, 'coneOuterAngle', 360),
            'coneOuterAngle'),
        coneOuterGain: spatialOuterGain(spatialOption(options, 'coneOuterGain', 0))
    });

    class PannerNode extends AudioNode {
        constructor(context, options = {}) {
            if (options == null) options = {};
            if (typeof options !== 'object')
                throw new TypeError('Panner options must be a dictionary');
            const state = spatialOptions(options);
            const params = Object.fromEntries(Object.entries({
                positionX: 0, positionY: 0, positionZ: 0,
                orientationX: 1, orientationY: 0, orientationZ: 0
            }).map(([name, fallback]) => [name,
                finiteFloat(spatialOption(options, name, fallback), name)]));
            super(audioNodeToken, context, 1, 1);
            spatialPannerState.set(this, state);
            for (const [name, initial] of Object.entries(params))
                spatialAudioParam(this, context, name, initial);
            audioContextState.get(context).panners.add(this);
            audioNodeState.get(this).render = (frame, frames, cache) =>
                renderSpatialPanner(this, frame, frames, cache);
        }
        get panningModel() { return spatialPannerState.get(this).panningModel; }
        set panningModel(value) {
            spatialPannerState.get(this).panningModel = spatialModel(value);
        }
        get distanceModel() { return spatialPannerState.get(this).distanceModel; }
        set distanceModel(value) {
            spatialPannerState.get(this).distanceModel = spatialDistanceModel(value);
        }
        get refDistance() { return spatialPannerState.get(this).refDistance; }
        set refDistance(value) {
            spatialPannerState.get(this).refDistance =
                spatialNumber(value, 'refDistance', 0);
        }
        get maxDistance() { return spatialPannerState.get(this).maxDistance; }
        set maxDistance(value) {
            spatialPannerState.get(this).maxDistance =
                spatialMaximumDistance(value);
        }
        get rolloffFactor() { return spatialPannerState.get(this).rolloffFactor; }
        set rolloffFactor(value) {
            spatialPannerState.get(this).rolloffFactor =
                spatialNumber(value, 'rolloffFactor', 0);
        }
        get coneInnerAngle() { return spatialPannerState.get(this).coneInnerAngle; }
        set coneInnerAngle(value) {
            spatialPannerState.get(this).coneInnerAngle =
                spatialNumber(value, 'coneInnerAngle');
        }
        get coneOuterAngle() { return spatialPannerState.get(this).coneOuterAngle; }
        set coneOuterAngle(value) {
            spatialPannerState.get(this).coneOuterAngle =
                spatialNumber(value, 'coneOuterAngle');
        }
        get coneOuterGain() { return spatialPannerState.get(this).coneOuterGain; }
        set coneOuterGain(value) {
            spatialPannerState.get(this).coneOuterGain =
                spatialOuterGain(value);
        }
        setPosition(x, y, z) {
            const values = [x, y, z].map((value, index) =>
                finiteFloat(value, 'source position ' + index));
            [this.positionX, this.positionY, this.positionZ].forEach((param, index) =>
                { param.value = values[index]; });
        }
        setOrientation(x, y, z) {
            const values = [x, y, z].map((value, index) =>
                finiteFloat(value, 'source orientation ' + index));
            [this.orientationX, this.orientationY, this.orientationZ].forEach((param, index) =>
                { param.value = values[index]; });
        }
    }

    const SPATIAL_POSITION = ['positionX', 'positionY', 'positionZ'];
    const SPATIAL_ORIENTATION = ['orientationX', 'orientationY', 'orientationZ'];
    const SPATIAL_FORWARD = ['forwardX', 'forwardY', 'forwardZ'];
    const SPATIAL_UP = ['upX', 'upY', 'upZ'];
    const spatialParamVariesInQuantum = (param, frame, frames, rate) => {
        const state = audioParamState.get(param);
        if (state.rate === 'k-rate') return false;
        if (state.inputs.size) return true;
        const start = frame / rate, end = (frame + frames) / rate;
        return state.events.some(event => event.type !== 'set' ||
            (event.time > start && event.time < end));
    };
    const spatialFrameGains = (node, frame, offset, cache, stereo) => {
        const listener = node.context.listener;
        const rate = node.context.sampleRate;
        const state = spatialPannerState.get(node);
        const source = spatialValues(node, SPATIAL_POSITION,
            frame, offset, rate, cache);
        const listenerPosition = spatialValues(listener, SPATIAL_POSITION,
            frame, offset, rate, cache);
        const forward = spatialValues(listener, SPATIAL_FORWARD,
            frame, offset, rate, cache);
        const up = spatialValues(listener, SPATIAL_UP,
            frame, offset, rate, cache);
        const orientation = spatialValues(node, SPATIAL_ORIENTATION,
            frame, offset, rate, cache);
        const azimuth = spatialAzimuth(source, listenerPosition, forward, up);
        const x = stereo ? (azimuth <= 0 ? (azimuth + 90) / 90 : azimuth / 90) :
            (azimuth + 90) / 180;
        const distance = spatialLength(spatialSubtract(source, listenerPosition));
        const gain = spatialDistanceGain(distance, state) *
            spatialConeGain(source, listenerPosition, orientation, state);
        return { azimuth, gain, gainL: Math.cos(x * Math.PI / 2),
            gainR: Math.sin(x * Math.PI / 2) };
    };

    // PannerNode's default input is clamped-max 2 with speaker interpretation.
    // Web Audio 1.0 §4.5 downmixes each connection before summing the input;
    // layouts other than mono, stereo, quad and 5.1 use discrete first channels.
    const spatialMixInput = (inputs, frames) => {
        const stereo = inputs.some(samples => samples.length > 1);
        const left = new Float32Array(frames);
        const right = stereo ? new Float32Array(frames) : null;
        for (const samples of inputs) {
            const count = samples.length;
            if (count === 4) {
                for (let i = 0; i < frames; i++) {
                    left[i] += 0.5 * (samples[0][i] + samples[2][i]);
                    right[i] += 0.5 * (samples[1][i] + samples[3][i]);
                }
            } else if (count === 6) {
                for (let i = 0; i < frames; i++) {
                    left[i] += samples[0][i] + Math.SQRT1_2 *
                        (samples[2][i] + samples[4][i]);
                    right[i] += samples[1][i] + Math.SQRT1_2 *
                        (samples[2][i] + samples[5][i]);
                }
            } else {
                for (let i = 0; i < frames; i++) {
                    left[i] += samples[0][i];
                    if (stereo) right[i] += (samples[1] ?? samples[0])[i];
                }
            }
        }
        return { left, right, stereo };
    };

    const renderSpatialPanner = (node, frame, frames, cache) => {
        const inputs = [...audioNodeState.get(node).inputs[0]].map(edge =>
            renderAudioNode(node.context, edge.source, frame, frames, cache,
                edge.output));
        const { left, right, stereo } = spatialMixInput(inputs, frames);
        const output = silence(2, frames);
        const listener = node.context.listener;
        const rate = node.context.sampleRate;
        const varies = [
            ...SPATIAL_POSITION.map(name => node[name]),
            ...SPATIAL_ORIENTATION.map(name => node[name]),
            ...SPATIAL_POSITION.map(name => listener[name]),
            ...SPATIAL_FORWARD.map(name => listener[name]),
            ...SPATIAL_UP.map(name => listener[name])
        ].some(param => spatialParamVariesInQuantum(param, frame, frames, rate));
        const fixed = varies ? null : spatialFrameGains(node, frame, 0, cache, stereo);
        for (let i = 0; i < frames; i++) {
            const { azimuth, gain, gainL, gainR } = fixed ||
                spatialFrameGains(node, frame, i, cache, stereo);
            if (stereo) {
                output[0][i] = gain * (azimuth <= 0 ?
                    left[i] + right[i] * gainL : left[i] * gainL);
                output[1][i] = gain * (azimuth <= 0 ?
                    right[i] * gainR : right[i] + left[i] * gainR);
            } else {
                output[0][i] = gain * left[i] * gainL;
                output[1][i] = gain * left[i] * gainR;
            }
        }
        return output;
    };
