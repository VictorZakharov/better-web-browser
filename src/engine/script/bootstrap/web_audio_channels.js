    const validAudioPorts = (value, name) => {
        value = Number(value) >>> 0;
        if (value < 1 || value > 32)
            throw new DOMException(name + ' must be between 1 and 32', 'IndexSizeError');
        return value;
    };

    // Each splitter output carries exactly one positional input channel. The
    // explicit input layout fills absent source channels with silence.
    class ChannelSplitterNode extends AudioNode {
        constructor(context, options = {}) {
            const outputs = validAudioPorts(options.numberOfOutputs === undefined ? 6 :
                options.numberOfOutputs, 'numberOfOutputs');
            super(audioNodeToken, context, 1, outputs);
            audioNodeState.get(this).render = (frame, frames, cache, output) => {
                const input = mixAudioInputs(this.context, this, frame, frames, cache,
                    outputs, 0, 'explicit');
                return [input[output]];
            };
        }
    }

    // A merger has independent mono inputs; an unconnected input contributes
    // one silent output channel rather than shrinking the output layout.
    class ChannelMergerNode extends AudioNode {
        constructor(context, options = {}) {
            const inputs = validAudioPorts(options.numberOfInputs === undefined ? 6 :
                options.numberOfInputs, 'numberOfInputs');
            super(audioNodeToken, context, inputs, 1);
            audioNodeState.get(this).render = (frame, frames, cache) => {
                const output = silence(inputs, frames);
                for (let input = 0; input < inputs; ++input) {
                    const mixed = mixAudioInputs(this.context, this, frame, frames,
                        cache, 1, input, 'explicit');
                    output[input].set(mixed[0]);
                }
                return output;
            };
        }
    }
