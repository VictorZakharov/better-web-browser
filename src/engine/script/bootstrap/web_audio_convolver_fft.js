    // Zero-latency uniform partitions for the first 1024 IR samples, followed
    // by 1024-sample partitions whose one-block IR offset hides their FFT work.
    // Each overlap-add block is rendered once; the history is bounded by IR size.
    const CONVOLVER_HEAD = 128;
    const CONVOLVER_TAIL = 1024;
    const MAX_CONVOLVER_IR_FRAMES = 131072;
    const MAX_CONVOLVER_BYTES = 32 * 1024 * 1024;

    const convolverSpectrum = (samples, offset, block) => {
        const real = new Float64Array(block * 2);
        const imag = new Float64Array(block * 2);
        const available = Math.max(0, Math.min(block, samples.length - offset));
        for (let i = 0; i < available; ++i) real[i] = samples[offset + i];
        audioFft(real, imag, false);
        return { real, imag };
    };
    const convolverStage = (impulse, offset, block, count, scale) => {
        if (!count) return null;
        const responses = impulse.map(channel => Array.from({ length: count }, (_, part) => {
            const samples = channel.subarray(offset + part * block,
                offset + (part + 1) * block);
            const scaled = Float64Array.from(samples, sample => sample * scale);
            return convolverSpectrum(scaled, 0, block);
        }));
        return { block, count, responses, history: Array.from({ length: 2 }, () =>
            new Array(count)), overlap: [new Float32Array(block), new Float32Array(block)] };
    };
    const convolverBankBytes = (length, channels) => {
        const head = Math.ceil(Math.min(length, CONVOLVER_TAIL) / CONVOLVER_HEAD);
        const tail = Math.ceil(Math.max(0, length - CONVOLVER_TAIL) / CONVOLVER_TAIL);
        // Complex spectra use two Float64 arrays; include the acquired IR copy.
        return (channels + 2) * (head * 2 * CONVOLVER_HEAD +
            tail * 2 * CONVOLVER_TAIL) * 16 + channels * length * 4 +
            2 * (CONVOLVER_HEAD + 3 * CONVOLVER_TAIL) * 4;
    };
    const createConvolverBank = (impulse, scale) => {
        const length = impulse[0].length;
        const headCount = Math.ceil(Math.min(length, CONVOLVER_TAIL) / CONVOLVER_HEAD);
        const tailCount = Math.ceil(Math.max(0, length - CONVOLVER_TAIL) / CONVOLVER_TAIL);
        return { head: convolverStage(impulse, 0, CONVOLVER_HEAD, headCount, scale),
            tail: convolverStage(impulse, CONVOLVER_TAIL, CONVOLVER_TAIL,
                tailCount, scale),
            tailInput: [new Float32Array(CONVOLVER_TAIL),
                new Float32Array(CONVOLVER_TAIL)],
            tailReady: [new Float32Array(CONVOLVER_TAIL),
                new Float32Array(CONVOLVER_TAIL)],
            paths: impulse.length === 1 ? [[[0, 0]], [[1, 0]]] :
                impulse.length === 2 ? [[[0, 0]], [[1, 1]]] :
                    [[[0, 0], [1, 2]], [[0, 1], [1, 3]]],
            blockIndex: 0, length, remaining: 0, idle: true };
    };

    const convolvePartition = (stage, input, index, paths) => {
        const { block, count, history, responses, overlap } = stage;
        const slot = index % count;
        for (let channel = 0; channel < 2; ++channel)
            history[channel][slot] = convolverSpectrum(input[channel], 0, block);
        const output = [];
        for (let channel = 0; channel < 2; ++channel) {
            const real = new Float64Array(block * 2);
            const imag = new Float64Array(block * 2);
            for (const [source, response] of paths[channel]) {
                const filters = responses[response];
                for (let part = 0; part < count && part <= index; ++part) {
                    const x = history[source][(index - part) % count];
                    if (!x) continue;
                    const h = filters[part];
                    for (let bin = 0; bin < real.length; ++bin) {
                        real[bin] += x.real[bin] * h.real[bin] -
                            x.imag[bin] * h.imag[bin];
                        imag[bin] += x.real[bin] * h.imag[bin] +
                            x.imag[bin] * h.real[bin];
                    }
                }
            }
            audioFft(real, imag, true);
            const samples = new Float32Array(block);
            for (let i = 0; i < block; ++i) {
                samples[i] = real[i] / (block * 2) + overlap[channel][i];
                overlap[channel][i] = real[block + i] / (block * 2);
            }
            output.push(samples);
        }
        return output;
    };
    const renderConvolverBank = (bank, input, frames) => {
        const index = bank.blockIndex++;
        let hasInput = false;
        for (const channel of input)
            for (let i = 0; i < frames; ++i)
                if (channel[i] !== 0) { hasInput = true; break; }
        if (hasInput) {
            bank.remaining = Math.ceil((bank.length + (bank.tail ? CONVOLVER_TAIL :
                CONVOLVER_HEAD)) / CONVOLVER_HEAD);
            bank.idle = false;
        } else if (bank.remaining) bank.remaining--;
        else {
            if (!bank.idle) {
                for (const stage of [bank.head, bank.tail]) {
                    if (!stage) continue;
                    for (const history of stage.history) history.fill(undefined);
                    for (const overlap of stage.overlap) overlap.fill(0);
                }
                for (const channel of bank.tailInput) channel.fill(0);
                for (const channel of bank.tailReady) channel.fill(0);
                bank.idle = true;
            }
            return null;
        }
        const head = convolvePartition(bank.head, input, index, bank.paths);
        const output = [head[0].subarray(0, frames), head[1].subarray(0, frames)];
        if (!bank.tail) return output;
        const offset = index % (CONVOLVER_TAIL / CONVOLVER_HEAD) * CONVOLVER_HEAD;
        for (let channel = 0; channel < 2; ++channel) {
            bank.tailInput[channel].set(input[channel].subarray(0, frames), offset);
            for (let i = 0; i < frames; ++i)
                output[channel][i] += bank.tailReady[channel][offset + i];
        }
        if (offset + CONVOLVER_HEAD === CONVOLVER_TAIL) {
            const tailIndex = Math.floor(index / (CONVOLVER_TAIL / CONVOLVER_HEAD));
            bank.tailReady = convolvePartition(bank.tail, bank.tailInput,
                tailIndex, bank.paths);
        }
        return output;
    };
