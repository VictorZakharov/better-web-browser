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
    const readSpatialOptions = options => {
        // Web IDL converts the derived dictionary in lexical order before the
        // constructor performs semantic validation or admits a graph node.
        const number = (name, fallback) => audioOption(options, name, fallback,
            value => finite(value, name));
        const param = (name, fallback) => audioOption(options, name, fallback,
            value => finiteFloat(value, name));
        return {
            coneInnerAngle: number('coneInnerAngle', 360),
            coneOuterAngle: number('coneOuterAngle', 360),
            coneOuterGain: number('coneOuterGain', 0),
            distanceModel: audioOption(options, 'distanceModel', 'inverse', value =>
                audioChannelEnum(value, ['linear', 'inverse', 'exponential'], 'distanceModel')),
            maxDistance: number('maxDistance', 10000),
            orientationX: param('orientationX', 1),
            orientationY: param('orientationY', 0),
            orientationZ: param('orientationZ', 0),
            panningModel: audioOption(options, 'panningModel', 'equalpower', value =>
                audioChannelEnum(value, ['equalpower', 'HRTF'], 'panningModel')),
            positionX: param('positionX', 0),
            positionY: param('positionY', 0),
            positionZ: param('positionZ', 0),
            refDistance: number('refDistance', 1),
            rolloffFactor: number('rolloffFactor', 1)
        };
    };
    const spatialOptions = options => ({
        panningModel: spatialModel(options.panningModel),
        distanceModel: spatialDistanceModel(options.distanceModel),
        refDistance: spatialNumber(options.refDistance, 'refDistance', 0),
        maxDistance: spatialMaximumDistance(options.maxDistance),
        rolloffFactor: spatialNumber(options.rolloffFactor, 'rolloffFactor', 0),
        coneInnerAngle: options.coneInnerAngle,
        coneOuterAngle: options.coneOuterAngle,
        coneOuterGain: spatialOuterGain(options.coneOuterGain)
    });

    class PannerNode extends AudioNode {
        constructor(context, options = {}) {
            const channels = readAudioChannelOptions(context, options);
            options = readSpatialOptions(channels.options);
            const state = spatialOptions(options);
            super(audioNodeToken, context, 1, 1,
                audioChannelSettings(context, channels, 'panner'));
            spatialPannerState.set(this, state);
            for (const name of ['positionX', 'positionY', 'positionZ',
                'orientationX', 'orientationY', 'orientationZ'])
                spatialAudioParam(this, context, name, options[name]);
            audioContextState.get(context).panners.add(this);
            audioNodeState.get(this).render = (frame, frames, cache) =>
                renderSpatialPanner(this, frame, frames, cache);
        }
        get panningModel() { return spatialPannerState.get(this).panningModel; }
        set panningModel(value) {
            const state = spatialPannerState.get(this);
            if (!state) throw new TypeError('Illegal PannerNode receiver');
            value = audioEnumAttribute(value, ['equalpower', 'HRTF']);
            if (value !== null) state.panningModel = spatialModel(value);
        }
        get distanceModel() { return spatialPannerState.get(this).distanceModel; }
        set distanceModel(value) {
            const state = spatialPannerState.get(this);
            if (!state) throw new TypeError('Illegal PannerNode receiver');
            value = audioEnumAttribute(value, ['linear', 'inverse', 'exponential']);
            if (value !== null) state.distanceModel = value;
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

    const renderSpatialPanner = (node, frame, frames, cache) => {
        const mixed = mixAudioInputs(node.context, node, frame, frames, cache);
        const [left, right] = mixed;
        const stereo = mixed.length === 2;
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
