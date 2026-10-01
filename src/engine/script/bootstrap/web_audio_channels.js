    const validAudioPorts = (value, name) => {
        value = audioChannelUnsigned(value);
        if (value < 1 || value > 32)
            throw new DOMException(name + ' must be between 1 and 32', 'IndexSizeError');
        return value;
    };

    // Each splitter output carries exactly one positional input channel. The
    // explicit input layout fills absent source channels with silence.
    class ChannelSplitterNode extends AudioNode {
        constructor(context, options = {}) {
            const channels = readAudioChannelOptions(context, options);
            const outputs = audioOption(channels.options, 'numberOfOutputs', 6,
                value => validAudioPorts(value, 'numberOfOutputs'));
            super(audioNodeToken, context, 1, outputs,
                audioChannelSettings(context, channels, 'splitter', outputs));
            audioNodeState.get(this).render = (frame, frames, cache, output) => {
                const input = mixAudioInputs(this.context, this, frame, frames, cache);
                return [input[output]];
            };
        }
    }

    // A merger has independent mono inputs; an unconnected input contributes
    // one silent output channel rather than shrinking the output layout.
    class ChannelMergerNode extends AudioNode {
        constructor(context, options = {}) {
            const channels = readAudioChannelOptions(context, options);
            const inputs = audioOption(channels.options, 'numberOfInputs', 6,
                value => validAudioPorts(value, 'numberOfInputs'));
            super(audioNodeToken, context, inputs, 1,
                audioChannelSettings(context, channels, 'merger', 1));
            audioNodeState.get(this).render = (frame, frames, cache) => {
                const output = silence(inputs, frames);
                for (let input = 0; input < inputs; ++input) {
                    const mixed = mixAudioInputs(this.context, this, frame, frames,
                        cache, undefined, input);
                    output[input].set(mixed[0]);
                }
                return output;
            };
        }
    }
