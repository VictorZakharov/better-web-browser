    const audioListenerToken = {};
    const audioListenerContext = new WeakMap();
    const SPATIAL_FLOAT_LIMIT = 3.4028234663852886e38;
    const spatialAudioParam = (owner, context, name, initial) => {
        const param = new AudioParam(audioParamToken, context, initial,
            -SPATIAL_FLOAT_LIMIT, SPATIAL_FLOAT_LIMIT);
        audioParamState.get(param).owner = owner;
        Object.defineProperty(owner, name, { enumerable: true, value: param });
    };

    class AudioListener {
        constructor(token, context) {
            if (token !== audioListenerToken) throw new TypeError('Illegal constructor');
            audioListenerContext.set(this, context);
            for (const [name, initial] of Object.entries({
                positionX: 0, positionY: 0, positionZ: 0,
                forwardX: 0, forwardY: 0, forwardZ: -1,
                upX: 0, upY: 1, upZ: 0
            })) spatialAudioParam(this, context, name, initial);
        }
        setPosition(x, y, z) {
            const values = [x, y, z].map((value, index) =>
                finiteFloat(value, 'listener position ' + index));
            [this.positionX, this.positionY, this.positionZ].forEach((param, index) =>
                { param.value = values[index]; });
        }
        setOrientation(x, y, z, upX, upY, upZ) {
            const values = [x, y, z, upX, upY, upZ].map((value, index) =>
                finiteFloat(value, 'listener orientation ' + index));
            [this.forwardX, this.forwardY, this.forwardZ,
                this.upX, this.upY, this.upZ].forEach((param, index) =>
                { param.value = values[index]; });
        }
    }
