    // Web Audio 1.0 §6: the coordinate system, equal-power azimuth, distance,
    // and cone calculations are shared by offline and live graph rendering.
    const spatialSubtract = (a, b) => a.map((value, i) => value - b[i]);
    const spatialDot = (a, b) => a.reduce((sum, value, i) => sum + value * b[i], 0);
    const spatialCross = (a, b) => [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0]
    ];
    const spatialLength = vector => Math.hypot(...vector);
    const spatialNormalize = vector => {
        const length = spatialLength(vector);
        return length ? vector.map(value => value / length) : null;
    };
    const spatialCosine = (a, b) =>
        Math.max(-1, Math.min(1, spatialDot(a, b)));
    const spatialValues = (owner, names, frame, offset, rate, cache) =>
        names.map(name => audioParamValueForFrame(owner[name], frame, offset,
            rate, cache));

    const spatialAzimuth = (source, listener, forward, listenerUp) => {
        const direction = spatialNormalize(spatialSubtract(source, listener));
        const right = spatialNormalize(spatialCross(forward, listenerUp));
        const front = spatialNormalize(forward);
        if (!direction || !right || !front) return 0;
        const up = spatialCross(right, front);
        const elevation = spatialDot(direction, up);
        const projected = spatialNormalize(direction.map((value, i) =>
            value - elevation * up[i]));
        if (!projected) return 0;
        let azimuth = Math.acos(spatialCosine(projected, right)) * 180 / Math.PI;
        if (spatialDot(projected, front) < 0) azimuth = 360 - azimuth;
        azimuth = azimuth <= 270 ? 90 - azimuth : 450 - azimuth;
        if (azimuth < -90) azimuth = -180 - azimuth;
        else if (azimuth > 90) azimuth = 180 - azimuth;
        return azimuth;
    };

    const spatialDistanceGain = (distance, state) => {
        const reference = state.refDistance, rolloff = state.rolloffFactor;
        let gain;
        if (state.distanceModel === 'linear') {
            const lower = Math.min(reference, state.maxDistance);
            const upper = Math.max(reference, state.maxDistance);
            // The linear model's nominal rolloff range is [0, 1]. The
            // exposed attribute remains unchanged when its DSP value clamps.
            const linearRolloff = Math.min(1, rolloff);
            gain = lower === upper ? 1 - linearRolloff : 1 - linearRolloff *
                (Math.max(lower, Math.min(distance, upper)) - lower) /
                (upper - lower);
        } else if (!reference) gain = 0;
        else if (state.distanceModel === 'inverse')
            gain = reference / (reference + rolloff *
                (Math.max(distance, reference) - reference));
        else gain = Math.pow(Math.max(distance, reference) / reference,
            -rolloff);
        return Math.max(0, Math.min(1, gain));
    };

    const spatialConeGain = (source, listener, orientation, state) => {
        if ((state.coneInnerAngle === 360 && state.coneOuterAngle === 360) ||
            !spatialLength(orientation)) return 1;
        const towardListener = spatialNormalize(spatialSubtract(listener, source));
        if (!towardListener) return 1;
        const angle = Math.acos(spatialCosine(
            towardListener, spatialNormalize(orientation))) * 180 / Math.PI;
        const inner = Math.abs(state.coneInnerAngle) / 2;
        const outer = Math.abs(state.coneOuterAngle) / 2;
        if (angle <= inner) return 1;
        if (angle >= outer) return state.coneOuterGain;
        const fraction = (angle - inner) / (outer - inner);
        return 1 - fraction + state.coneOuterGain * fraction;
    };
