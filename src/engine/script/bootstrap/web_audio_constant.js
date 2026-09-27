    // Web Audio 1.0 §1.16: a scheduled mono signal whose offset is an a-rate parameter.
    class ConstantSourceNode extends AudioScheduledSourceNode {
        constructor(context, options = {}) {
            const offset = options.offset === undefined ? 1 :
                finiteFloat(options.offset, 'offset');
            super(audioNodeToken, context);
            Object.defineProperty(this, 'offset', { enumerable: true,
                value: new AudioParam(audioParamToken, context,
                    offset,
                    -3.4028234663852886e38, 3.4028234663852886e38) });
            audioParamState.get(this.offset).owner = this;
            audioNodeState.get(this).render = (frame, frames, cache) => {
                const output = silence(1, frames)[0];
                const schedule = audioScheduledState.get(this);
                if (schedule.start === null) return [output];
                for (let i = 0; i < frames; ++i) {
                    const time = (frame + i) / this.context.sampleRate;
                    if (time < schedule.start) continue;
                    if (time >= schedule.stop) { finishAudioSource(this); break; }
                    output[i] = audioParamValueForFrame(this.offset, frame, i,
                        this.context.sampleRate, cache);
                }
                return [output];
            };
        }
    }
