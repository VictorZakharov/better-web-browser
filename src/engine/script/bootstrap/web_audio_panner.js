    // Web Audio 1.0 §6.3.3: equal-power panning, with distinct mono/stereo equations.
    class StereoPannerNode extends AudioNode {
        constructor(context, options = {}) {
            const pan = options.pan === undefined ? 0 : finiteFloat(options.pan, 'pan');
            super(audioNodeToken, context, 1, 1);
            Object.defineProperty(this, 'pan', { enumerable: true,
                value: new AudioParam(audioParamToken, context,
                    pan, -1, 1) });
            audioParamState.get(this.pan).owner = this;
            audioNodeState.get(this).render = (frame, frames, cache) => {
                const inputs = [...audioNodeState.get(this).inputs[0]].map(edge =>
                    renderAudioNode(this.context, edge.source, frame, frames, cache,
                        edge.output));
                const stereo = inputs.some(samples => samples.length > 1);
                const mixed = silence(stereo ? 2 : 1, frames);
                for (const samples of inputs) {
                    for (let i = 0; i < frames; ++i) {
                        if (stereo) {
                            mixed[0][i] += samples[0][i];
                            mixed[1][i] += samples.length === 1 ? samples[0][i] : samples[1][i];
                        } else mixed[0][i] += samples[0][i];
                    }
                }
                const output = silence(2, frames);
                for (let i = 0; i < frames; ++i) {
                    const pan = Math.max(-1, Math.min(1,
                        audioParamValueForFrame(this.pan, frame, i,
                            this.context.sampleRate, cache)));
                    const x = stereo ? (pan <= 0 ? pan + 1 : pan) : (pan + 1) / 2;
                    const gainL = Math.cos(x * Math.PI / 2);
                    const gainR = Math.sin(x * Math.PI / 2);
                    if (stereo) {
                        if (pan <= 0) {
                            output[0][i] = mixed[0][i] + mixed[1][i] * gainL;
                            output[1][i] = mixed[1][i] * gainR;
                        } else {
                            output[0][i] = mixed[0][i] * gainL;
                            output[1][i] = mixed[1][i] + mixed[0][i] * gainR;
                        }
                    } else {
                        output[0][i] = mixed[0][i] * gainL;
                        output[1][i] = mixed[0][i] * gainR;
                    }
                }
                return output;
            };
        }
    }
